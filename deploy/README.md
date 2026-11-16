# Installation sur le serveur Debian (Docker)

Destinataires : IT / DSI. Prérequis : serveur préparé selon les étapes 1 à 10 du README principal (Debian 13, SSH par clé, ufw, Docker Engine + plugin compose, IP réservée).

L'application tourne en 4 conteneurs : `db` (PostgreSQL 17), `app` (API + interface web), `worker` (avis, PDF en attente), `proxy` (Caddy, HTTPS). Seul `proxy` publie des ports (80 et 443). Les données sont dans des volumes Docker : `celluloplast_pgdata`, `celluloplast_documents`, `celluloplast_caddy_data`.

## 1. Récupérer le code et configurer

```bash
sudo mkdir -p /opt/celluloplast && sudo chown "$USER" /opt/celluloplast
git clone https://github.com/plandevdz/celluloRHIT.git /opt/celluloplast
cd /opt/celluloplast
cp .env.example .env && chmod 600 .env
sed -i "s/^POSTGRES_PASSWORD=.*/POSTGRES_PASSWORD=$(openssl rand -hex 24)/" .env
nano .env   # SERVER_ADDRESS et PUBLIC_URL = IP réservée du serveur (ou nom DNS interne)
```

## 2. Construire et démarrer

```bash
sudo docker compose up -d --build
sudo docker compose ps
sudo docker compose logs -f app     # « API démarrée », migrations appliquées
```

La première construction compile le serveur Rust (10 à 20 minutes sur un vieux PC).

## 3. Créer le premier administrateur

```bash
sudo docker compose exec app celluloplast create-admin --email prenom.nom@celluloplast.dz --name "Prénom Nom"
```

La commande affiche un **lien d'activation à usage unique (72 h)**. Ouvrez-le dans le navigateur pour choisir le mot de passe. Ensuite, dans **Administration** : créez les sites et les locaux, puis les utilisateurs et leurs rôles. Chaque création affiche un lien d'activation à transmettre à la personne.

## 4. Faire confiance au certificat sur les PC Windows

Caddy délivre le certificat HTTPS depuis sa propre autorité interne. Exportez son certificat racine :

```bash
sudo docker compose cp proxy:/data/caddy/pki/authorities/local/root.crt /tmp/celluloplast-root.crt
sudo chmod 644 /tmp/celluloplast-root.crt   # certificat public : lisible pour le copier par scp
```

Depuis le PC : `scp it1@<IP>:/tmp/celluloplast-root.crt .`

Installez `celluloplast-root.crt` sur chaque PC dans **Autorités de certification racines de confiance** :
- par GPO : Configuration ordinateur → Paramètres Windows → Paramètres de sécurité → Stratégies de clé publique → Autorités de certification racines de confiance → Importer ;
- ou manuellement, en PowerShell administrateur : `certutil -addstore -f Root celluloplast-root.crt`.

Ensuite `https://<IP-du-serveur>/` s'ouvre dans Edge ou Chrome sans avertissement. Ne demandez jamais aux utilisateurs d'ignorer un avertissement de certificat. Pour utiliser plutôt un certificat de la PKI d'entreprise, remplacez `tls internal` par `tls /chemin/cert.pem /chemin/cle.pem` dans `deploy/Caddyfile`.

## 5. Pare-feu

Docker publie 80 et 443 en contournant les règles `ufw INPUT`. Pour limiter l'accès aux sous-réseaux des utilisateurs, posez la règle dans la chaîne `DOCKER-USER` (exemple pour un LAN 10.0.0.0/24) :

```bash
sudo iptables -I DOCKER-USER -p tcp -m multiport --dports 80,443 ! -s 10.0.0.0/24 -j DROP
```

Rendez la règle persistante (paquet `iptables-persistent`) ou appliquez le filtrage sur le pare-feu réseau. PostgreSQL n'est jamais publié.

## Mise à jour

```bash
cd /opt/celluloplast
sudo deploy/backup.sh /var/backups/celluloplast        # toujours avant une mise à jour
git pull
sudo docker compose up -d --build
```

Les migrations SQL s'appliquent au démarrage de `app`. Les navigateurs ouverts affichent « nouvelle version disponible » et proposent de recharger la page.

## Sauvegarde et restauration

### Ce que couvre `deploy/backup.sh`

| Élément | Fichier produit | Remarque |
|---|---|---|
| Base PostgreSQL : dossiers, rôles, audit, références, empreintes des documents | `cellulo-<date>.dump.gpg` | `pg_dump` en format custom, chiffré |
| Décharges PDF originales, scans signés | `documents-<date>.tar.gz.gpg` | volume `celluloplast_documents`, chiffré |
| Configuration `.env` + autorité de certification interne de Caddy (certificat racine et **clé privée**) | `secrets-<date>.tar.gz.gpg` | **toujours chiffré, jamais écrit en clair**. Sans cette CA, une restauration crée une nouvelle autorité et tous les PC Windows doivent réinstaller le certificat racine. |
| Empreintes SHA-256 des fichiers ci-dessus | `sha256-<date>.txt` | à vérifier avant restauration |
| Code et image Docker | — | reconstruits depuis Git (`docker compose up -d --build`) |

**Chiffrement dès l'écriture, à clé publique.** Les flux passent de `pg_dump` ou `tar` directement à `gpg`, sans fichier intermédiaire en clair. Le serveur ne détient que la **clé publique** de récupération ; la **clé privée** reste hors du serveur (coffre de la DSI). Une personne qui copierait les sauvegardes depuis le serveur ne pourrait donc pas les déchiffrer.

**Sans clé configurée**, le script :
- ne crée **aucune** archive de secrets ;
- écrit la base et les documents en clair ;
- affiche « SAUVEGARDE INCOMPLÈTE » et se termine avec le **code 3**.

Un cron doit traiter ce code comme une alerte. `--sans-secrets` exclut volontairement `.env` et la CA : sauvegarde « partielle », code 0.

#### Mettre en place la clé de récupération (une fois)

Sur un **poste d'administration**, pas sur le serveur (Linux avec gnupg, ou Windows avec Gpg4win) :

```bash
gpg --quick-generate-key "Celluloplast sauvegarde (DSI)" rsa4096 encr 5y        # phrase de passe forte
gpg --list-keys --with-colons "Celluloplast sauvegarde" | awk -F: '$1=="fpr"{print $10; exit}'   # empreinte
gpg --armor --export <EMPREINTE> > backup-pubkey.asc                             # partie PUBLIQUE
gpg --armor --export-secret-keys <EMPREINTE> > cle-privee-sauvegarde.asc         # à ranger HORS LIGNE (coffre)
gpg --output revocation.asc --gen-revoke <EMPREINTE>                             # certificat de révocation, au coffre
```

Sur le serveur, ne copiez **que** la partie publique :

```bash
sudo install -m 644 backup-pubkey.asc /etc/celluloplast/backup-pubkey.asc
echo 'BACKUP_GPG_RECIPIENT=<EMPREINTE>' | sudo tee /etc/celluloplast/backup.conf
sudo chmod 600 /etc/celluloplast/backup.conf
sudo deploy/backup.sh /var/backups/celluloplast; echo "code=$?"   # attendu : « Sauvegarde COMPLÈTE », code=0
```

Le script refuse de démarrer si l'empreinte ne correspond pas à la clé publique fournie. Planifiez-le chaque jour (cron root) et copiez le résultat **hors de la machine** (`rsync -a /var/backups/celluloplast/ sauvegarde@serveur-de-sauvegarde:/celluloplast/`). Un disque unique n'est pas une sauvegarde. Aucun de ces fichiers ne doit être déposé dans Git.

Test automatisé, avec des secrets fictifs et un faux `docker` : `bash deploy/tests/test-backup.sh`. Il vérifie le chiffrement, l'absence de tout secret en clair sur disque, le déchiffrement avec la seule clé privée, le code 3 sans configuration et le refus d'une empreinte incohérente.

Pour remplacer plutôt la CA interne par un certificat de la PKI d'entreprise, mettez `tls /certs/cert.pem /certs/key.pem` dans `deploy/Caddyfile` et montez ces fichiers en lecture seule dans le service `proxy`. Les PC qui font confiance à la PKI d'entreprise n'ont alors rien à installer.

### Recette de restauration isolée (`deploy/restore-isole.sh`)

**Pour cette première recette, une VM ou une machine de test distincte de la production est obligatoire.** Ne jamais la jouer sur le serveur de production. Le script refuse de s'exécuter dans les cas suivants :
- conteneurs ou volumes du projet de production `celluloplast` présents ;
- projet de recette `cellulo-restore` déjà existant (cible non vide) ;
- base cible non vide ;
- adresse fournie identique à celle de la production ;
- empreinte de CA différente de celle attendue.

Chaque commande Docker utilise explicitement `-p cellulo-restore`. Le script s'arrête à la première étape en échec, **avant** le démarrage de l'application et **avant** tout nettoyage des fichiers.

Prérequis sur la VM de test :
- Debian, Docker et le plugin compose installés ;
- dépôt cloné dans `/opt/celluloplast`, sur le SHA à recetter ;
- un compte nominatif dont `sudo` demande un mot de passe.

**1. Sur le poste d'administration** (qui détient la clé privée de récupération). Les fichiers déchiffrés sont écrits dans le répertoire du compte nominatif de la VM, **sans sudo** : la commande distante n'a pas besoin de terminal.

```bash
set -euo pipefail                      # toute commande en échec arrête la suite
B=/chemin/des/sauvegardes; D=AAAAMMJJ-HHMMSS; H=prenom.nom@<IP-de-la-VM>
cd "$B" && sha256sum -c sha256-$D.txt  # intégrité des fichiers chiffrés
ssh "$H" 'umask 077 && mkdir -p ~/restau'
gpg -d cellulo-$D.dump.gpg       | ssh "$H" 'umask 077 && cat > ~/restau/cellulo.dump'
gpg -d documents-$D.tar.gz.gpg   | ssh "$H" 'umask 077 && cat > ~/restau/documents.tar.gz'
gpg -d secrets-$D.tar.gz.gpg     | ssh "$H" 'umask 077 && cat > ~/restau/secrets.tar.gz'
# Empreinte de la CA de production, à comparer (certificat installé sur les PC) :
openssl x509 -noout -fingerprint -sha256 -in celluloplast-root.crt
```

**2. Sur la VM de test**, en session interactive (`ssh -t`). `sudo` y demande le mot de passe du compte nominatif ; le script, exécuté en root, lit les fichiers de `~/restau` :

```bash
cd /opt/celluloplast
sudo deploy/restore-isole.sh --source ~/restau --adresse <IP-de-la-VM> \
     --machine-de-test-confirmee --empreinte-ca-attendue <EMPREINTE-SHA256-PRODUCTION>
```

Étapes du script :
1. garde-fous ;
2. empreinte de la CA sauvegardée ;
3. documents et CA dans des volumes **neufs** du projet `cellulo-restore` ;
4. base démarrée, vérifiée vide (`cellulo|0`), puis `pg_restore --exit-on-error` (le dump est lu par root) ;
5. démarrage de l'application en mode journal ;
6. contrôles :
   - nombre et références des dossiers ;
   - rôles par compte ;
   - empreintes SHA-256 de **chaque** PDF et scan (valeur en base = fichier restauré) ;
   - empreinte du **certificat racine Caddy avant/après** démarrage (Caddy ne doit pas en régénérer un) ;
   - `MAIL_MODE=journal` et état des remises d'avis ;
7. après succès seulement, effacement des fichiers en clair de l'espace de travail. `--supprimer-source` efface aussi `~/restau`.

Le script crée un environnement de recette `/root/cellulo-restore.env` (adresse de la VM, mot de passe PostgreSQL neuf, sans destinataire de test). Les contrôles manuels et le démontage l'utilisent, toujours avec le projet explicite :

```bash
E=/root/cellulo-restore.env
sudo docker compose -p cellulo-restore --env-file $E ps
sudo docker compose -p cellulo-restore --env-file $E exec -T db psql -U cellulo -d cellulo -c "select reference, status from cases order by created_at desc limit 5"
sudo docker compose -p cellulo-restore --env-file $E exec -T db psql -U cellulo -d cellulo -c "select u.email, string_agg(r.role, ' + ') from users u join user_roles r on r.user_id = u.id group by u.email"
sudo docker compose -p cellulo-restore --env-file $E exec -T db psql -U cellulo -d cellulo -c "select storage_path, file_kind, sha256 from document_versions order by created_at desc limit 4"
sudo docker compose -p cellulo-restore --env-file $E exec -T app sha256sum /var/lib/celluloplast/<storage_path>
sudo docker compose -p cellulo-restore --env-file $E exec -T db psql -U cellulo -d cellulo -c "select status, count(*) from deliveries group by status"
sudo docker compose -p cellulo-restore --env-file $E down -v      # démontage après recette
```

Test automatisé du script, sans Docker : `bash deploy/tests/test-restore.sh` (faux `docker` et faux `id`). Il vérifie :
- chaque garde-fou ;
- l'ordre des étapes ;
- l'arrêt avant démarrage si `pg_restore` échoue ;
- la conservation des fichiers en cas d'échec ;
- la détection d'une CA régénérée ;
- la présence de `-p cellulo-restore` sur chaque commande compose.

Il ne remplace pas la recette sur une vraie VM.

La **reprise en production** sur une nouvelle machine (projet `celluloplast`, `.env` d'origine) se fera selon une procédure dédiée, à rédiger **après** le succès de cette recette.

**État de vérification** (12/11/2026) :
- chiffrement et déchiffrement par `backup.sh` : **testés** avec des secrets fictifs et un faux `docker` (`deploy/tests/test-backup.sh`, 21 contrôles réussis) ;
- logique de `restore-isole.sh` : **testée** avec un faux `docker` (`deploy/tests/test-restore.sh`, 22 contrôles réussis) ;
- restauration PostgreSQL + documents : **exécutée et réussie** hors Docker (pg_dump/pg_restore vers une base neuve, dossier de documents neuf ; dossier, rôles et empreintes PDF et scan identiques ; voir 05-Recette.md, « RP ») ;
- **recette complète sur une VM avec un vrai Docker** (`backup.sh` puis `restore-isole.sh` ; comparaison du dossier, des rôles, des empreintes PDF/scan et du certificat racine Caddy avant/après) : **NON EXÉCUTÉE**. Docker n'est pas utilisable sur le poste de développement, et la production est exclue.

## Démonstration (données fictives uniquement)

Sur une instance de test **vide** :

```bash
sudo docker compose exec -e DEMO_PASSWORD='MotDePasseDemo-2026' app celluloplast seed-demo
```

Cette commande crée deux sites, un compte par rôle (dont un IT + FORMATION), du matériel, une SIM et un catalogue de formations. Elle refuse de s'exécuter si des salariés existent déjà.

## État de cette version

**Livré** :
- comptes et rôles ; annonce d'arrivée, besoins et révisions ; file « Mon travail » avec prise atomique ;
- formations (catalogue détaillé, évaluation) ; comptes IT et indicateur M365 ;
- parc IT : actifs (marque, modèle, Intune, garantie, état, matériel partagé « en service »), accessoires, SIM et lignes (forfait, type) ;
- remises et restitutions partielles avec décharge PDF et scan signé ; demandes ponctuelles et clôture contrôlée ;
- lot 1 (données filtrées par équipe, « Confirmer l'arrivée », fin de tâche avec preuve, avis honnêtes, sauvegarde) et lot 2 (Excel du personnel) ;
- **référentiels** : profils de poste administrables (besoins distincts avec quantités — PC, écran, tablette, imprimante, accessoire, téléphone, SIM — et formations suggérées à l'annonce), wilayas multiples et filtrables, contrat, manager relié, ligne principale, « en départ » ;
- **Excel du personnel v2** : contrat, profil, manager (matricule ou ID) et wilayas exportés et réimportés sans perte ; les anciens classeurs à huit colonnes restent acceptés ;
- **logiciels** : abonnements ou licences perpétuelles, sièges attribués sous verrou, libération selon le contrat, alerte de renouvellement à l'IT ;
- **véhicules RH** : propriété ou location, contrats renouvelés sans effacement, pièces (assurance, contrôle technique…), entretien par date ou kilométrage, remise, restitution et retour au loueur distincts, alertes d'échéance au RH ;
- **étiquettes QR** (planche A4 ou unitaire) et **campagnes d'inventaire** (poste de scan, écarts, arbitrages, rapport Excel) ;
- **mutation (M05)** : comparaison « Actuel / Cible / Action », application unique à la date d'effet, suspension si le poste a changé, report, annulation, correction datée ;
- **départ (M06)** : dernier jour et instant de coupure séparés, accès et biens à traiter, coupure confirmée par l'IT, relance « accès à vérifier », sortie administrative sans attendre les restitutions, écart, perte motivée ;
- **avis par Exchange Online** (Microsoft Graph, `MAIL_MODE=graph`) et **archivage SharePoint** (`ARCHIVE_MODE=sharepoint`) — désactivés par défaut ;
- **reprise des listes SharePoint** (Administration → Reprise SharePoint) ;
- **états des lieux** (menu « États ») exportables en Excel.

**À valider sur site** : impression des étiquettes et lecture à la douchette, impression A4 des décharges, rendu des noms arabes dans les PDF, raccordement réel d'Exchange et de SharePoint (comptes Microsoft à créer par l'administrateur), recette de restauration sur une VM.

## Passer à la version « lot 3 » (migrations 0003 à 0008)

Les migrations `0003` à `0008` sont **additives** : nouvelles tables (wilayas, profils, véhicules, logiciels, inventaires, mutations, départs, reprise) et nouvelles colonnes facultatives. Elles élargissent deux contraintes (types et états d'actif, nature de mouvement) sans changer de valeur existante. Aucune donnée n'est réinterprétée ; aucun e-mail n'est envoyé tant que `MAIL_MODE` reste à `journal`.

```bash
cd /opt/celluloplast
sudo deploy/backup.sh /var/backups/celluloplast          # sauvegarde AVANT (obligatoire)
sudo git fetch && sudo git checkout <version validée>
sudo docker compose up -d --build                        # migrations 0003 à 0008 au démarrage d'app
sudo docker compose logs app | grep -i -E "migr|démarrée|error"
```

Le `.env` existant reste valable : les nouvelles variables ont des valeurs par défaut (voir `.env.example`). Les fiches et dossiers existants ne reçoivent aucune valeur rétroactive : profil, contrat, wilayas et ligne principale restent vides (« — ») jusqu'à saisie ou reprise. Les classeurs Excel exportés avant cette version se réimportent tels quels. Retour arrière : comme pour la 0002, restaurer la sauvegarde prise juste avant.

## Mode pilote : fonctions ouvertes

`MODULES_DESACTIVES=pilote` est la valeur par défaut du `docker-compose.yml`. Il ferme les véhicules, les logiciels, les étiquettes et campagnes QR, la mutation, le départ et les états :
- menus et boutons masqués ;
- liens directs : « Fonction non ouverte » ;
- API refusée ;
- échéances, mutations et coupures non traitées par le worker.

Ouvrir un module après sa recette : retirer son nom de la liste (ex. `MODULES_DESACTIVES=vehicules,etats`), puis `docker compose up -d`. Une faute de frappe empêche le démarrage : un module ne reste jamais ouvert par erreur. La règle du pilote (seules les nouvelles opérations) est décrite dans `docs/pilote.md`.

## Raccorder Exchange Online (envoi des avis)

À faire avec l'administrateur Microsoft 365, d'abord vers une **boîte de test** :

1. Créer la boîte d'expédition dédiée (ex. `avis-parcours@…`).
2. Créer une application Entra « Celluloplast — envoi » ; lui associer un **certificat** (pas de secret partagé). Générer la paire sur un poste d'administration :
   `openssl req -x509 -newkey rsa:2048 -nodes -days 730 -subj "/CN=celluloplast-envoi" -keyout graph-envoi.key -out graph-envoi.crt`
   puis téléverser `graph-envoi.crt` dans l'application Entra.
3. Donner le droit **Application Mail.Send limité à cette seule boîte** (Exchange « Application RBAC ») ; vérifier qu'aucun octroi Entra plus large n'existe. Tester le refus sur une seconde boîte.
4. Sur le serveur : `sudo mkdir -m 700 /opt/celluloplast/secrets`, y copier `graph-envoi.crt` et `graph-envoi.key`, puis `sudo chown 10001 secrets/* && sudo chmod 600 secrets/*`.
5. Dans `.env` : `MAIL_MODE=graph`, `MAIL_SENDER=<boîte>`, `GRAPH_TENANT_ID`, `GRAPH_CLIENT_ID`, et pendant le pilote `MAIL_ALLOWED_DOMAINS=<domaine>` et/ou `MAIL_TEST_RECIPIENT=<boîte de test>`. Puis `sudo docker compose up -d`.
Les avis créés pendant la période en mode journal (tests, pilote) ne partent jamais lors de l'activation : ils sont consignés « avis antérieur à l'activation de l'envoi ».
6. Contrôler dans Administration → « Archivage et paramètres » et « Avis et mails » : « Accepté par Exchange » n'est pas une preuve de livraison ; « à vérifier » signale un résultat inconnu, jamais réexpédié automatiquement.

## Raccorder l'archivage SharePoint

Identité **distincte** de celle d'envoi : application Entra « Celluloplast — archive » avec certificat (`graph-archive.crt` / `graph-archive.key` dans `./secrets`), permission **Sites.Selected** accordée sur le seul site cible (tester le refus sur un autre site). Dans `.env` : `ARCHIVE_MODE=sharepoint`, `ARCHIVE_TENANT_ID`, `ARCHIVE_CLIENT_ID`, `ARCHIVE_SITE_ID`, `ARCHIVE_DRIVE_ID` (identifiants Graph du site et de la bibliothèque), `ARCHIVE_ROOT`. Les documents restent aussi en copie locale ; une panne est reprise automatiquement (suivi dans Administration → « Archivage et paramètres »).

## Reprendre les données SharePoint

Sur une **instance de test** d'abord (reprise à blanc), puis au gel final :

1. Créer les sites en Administration (les libellés doivent correspondre à la colonne « Site » de la liste Personnel).
2. Exporter chaque liste SharePoint en CSV ou en classeur Excel, **avec la colonne ID** si possible.
3. Administration → Reprise SharePoint : charger les listes dans l'ordre proposé (Wilayas, CatalogueFormations, UtilisateursApp, ProfilsPoste, Personnel, MaterielIT, LignesMobiles, Affectations, Formations, Vehicules, AffectationsVehicules).
4. Pour chaque lot : lire l'aperçu, décider les lignes « à revoir » (homonymes, rapprochements par le nom), puis appliquer. Les rejets se corrigent dans le fichier, rechargé ensuite comme nouveau lot.
5. Les comptes repris n'ont pas de mot de passe : générer leur lien d'activation (Administration → Utilisateurs).

Aucune arrivée, tâche ni avis n'est créé par une reprise ; les gardes reprises apparaissent comme mouvements « reprise » sans décharge. Les dossiers, tâches et modèles de tâches SharePoint ne sont pas repris : les parcours en cours se recréent dans l'application, l'ancienne application restant consultable en lecture seule.

## Passer à la version « lot 1 + lot 2 » (migration 0002)

La migration `0002_arrivee_preuves_import.sql` est **additive** :
- nouvelles colonnes sur `employees`, `tasks` et `outbox` ;
- nouvelles tables `import_batches` et `import_rows`.

Elle ne modifie aucune donnée existante :
- les salariés « à venir » le restent. Ceux dont la date prévue est passée, ou dont l'arrivée est close, apparaissent dans « Présence à confirmer » : c'est au RH de vérifier ;
- les tâches déjà terminées gardent une résolution vide (« terminée avant le contrôle des preuves »). Leurs anciennes observations ne deviennent pas des preuves ;
- les nouvelles fins de tâche exigent une résolution (contrainte `NOT VALID`, qui ne s'applique qu'aux écritures futures).

Sur une **instance de test** d'abord, puis en production :

```bash
cd /opt/celluloplast
sudo deploy/backup.sh /var/backups/celluloplast          # sauvegarde AVANT (obligatoire pour revenir en arrière)
git fetch && git checkout <version validée>
sudo docker compose up -d --build                        # la migration 0002 s'applique au démarrage d'app
sudo docker compose logs app | grep -i -E "migr|démarrée|error"
```

**Retour arrière.** Une fois la `0002` appliquée, l'ancienne image refuse de démarrer, car sqlx détecte une migration qu'elle ne connaît pas. Ne supprimez pas de colonnes à la main. Restaurez plutôt la sauvegarde prise juste avant (section « Restauration »), après être revenu au code précédent (`git checkout <version précédente>`, puis `docker compose up -d --build`). Les saisies faites entre la mise à jour et le retour arrière sont perdues : planifiez la mise à jour hors des heures de travail.
