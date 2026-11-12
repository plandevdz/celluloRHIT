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

### Restauration (instance isolée, volumes neufs)

Le déchiffrement se fait sur le **poste d'administration** qui détient la clé privée. Les flux déchiffrés sont envoyés à la machine de restauration par SSH, sans jamais y déposer la clé privée. On restaure sur une autre machine, ou sous un autre nom de projet pour ne pas toucher la production. L'envoi de mails reste en mode journal.

```bash
# --- sur le poste d'administration (clé privée importée : gpg --import cle-privee-sauvegarde.asc)
B=/chemin/des/sauvegardes; D=AAAAMMJJ-HHMMSS; H=admin@machine-de-restauration
cd "$B" && sha256sum -c sha256-$D.txt                                                   # 1. intégrité
gpg -d secrets-$D.tar.gz.gpg   | ssh $H 'sudo mkdir -p -m 700 /root/restau && sudo tar xzf - -C /root/restau'
gpg -d cellulo-$D.dump.gpg     | ssh $H 'sudo tee /root/restau/cellulo.dump >/dev/null'
gpg -d documents-$D.tar.gz.gpg | ssh $H 'sudo tee /root/restau/documents.tar.gz >/dev/null'

# --- sur la machine de restauration (dans /opt/celluloplast)
export COMPOSE_PROJECT_NAME=cellulo-restore
sudo install -m 600 /root/restau/cfg/.env .env                                         # 2. configuration
sudo docker compose up -d db                                                           # 3. base
sudo docker compose exec -T db pg_restore -U cellulo -d cellulo --clean --if-exists < /root/restau/cellulo.dump
sudo docker run --rm -v ${COMPOSE_PROJECT_NAME}_documents:/data -v /root/restau:/r debian:trixie-slim \
  sh -c "tar xzf /r/documents.tar.gz -C /data && chown -R 10001 /data"                # 4. documents
sudo docker run --rm -v ${COMPOSE_PROJECT_NAME}_caddy_data:/caddy -v /root/restau:/r debian:trixie-slim \
  sh -c "mkdir -p /caddy/caddy && cp -a /r/caddy/caddy/pki /caddy/caddy/"            # 5. même CA (même certificat racine)
sudo docker compose up -d --build                                                      # 6. application
sudo shred -u /root/restau/cfg/.env /root/restau/caddy/caddy/pki/authorities/local/*.key; sudo rm -rf /root/restau
```

Avant toute remise en service, contrôlez :

```bash
# un dossier et ses rôles
sudo docker compose exec db psql -U cellulo -c "select reference, status from cases order by created_at desc limit 5"
sudo docker compose exec db psql -U cellulo -c "select u.email, array_agg(r.role) from users u join user_roles r on r.user_id = u.id group by u.email"
# empreintes d'un PDF et d'un scan : la valeur en base doit égaler le fichier restauré
sudo docker compose exec db psql -U cellulo -c "select storage_path, file_kind, sha256 from document_versions order by created_at desc limit 4"
sudo docker compose exec app sha256sum /var/lib/celluloplast/<storage_path>
# file des avis : rien ne doit partir vers de vraies boîtes
sudo docker compose exec db psql -U cellulo -c "select status, count(*) from deliveries group by status"
```

**État de vérification** (12/11/2026) :
- chiffrement et déchiffrement par `backup.sh` : **testés** avec des secrets fictifs et un faux `docker` (`deploy/tests/test-backup.sh`, 21 contrôles réussis) ;
- restauration PostgreSQL + documents : **exécutée et réussie** hors Docker (pg_dump/pg_restore vers une base neuve, dossier de documents neuf ; dossier, rôles et empreintes PDF et scan identiques ; voir 05-Recette.md, « RP ») ;
- restauration complète par volumes Docker (procédure ci-dessus, CA Caddy comprise) et exécution de `backup.sh` contre un vrai Docker : **non exécutées**, faute de Docker utilisable sur le poste de développement. À jouer sur une machine de test avant la mise en production.

## Démonstration (données fictives uniquement)

Sur une instance de test **vide** :

```bash
sudo docker compose exec -e DEMO_PASSWORD='MotDePasseDemo-2026' app celluloplast seed-demo
```

Cette commande crée deux sites, un compte par rôle (dont un IT + FORMATION), du matériel, une SIM et un catalogue de formations. Elle refuse de s'exécuter si des salariés existent déjà.

## État de cette version

**Livré** :
- comptes et rôles ;
- annonce d'arrivée, besoins et révisions ;
- file « Mon travail » avec prise atomique ;
- formations ;
- comptes IT et indicateur M365 ;
- parc IT (actifs, accessoires, SIM) ;
- remises et restitutions partielles avec décharge PDF et scan signé ;
- demandes ponctuelles et clôture contrôlée ;
- avis en **mode journal** (aucun e-mail envoyé).

**Lot 1** (fiabilisation de l'arrivée) :
- données filtrées par équipe dans l'API ;
- « Confirmer l'arrivée » ;
- fin de tâche avec preuve (remis / déjà détenu / exception motivée) ;
- avis affichés honnêtement ;
- sauvegarde de la configuration et de la CA.

**Lot 2** : import et export Excel du personnel.

**Pas encore livré** :
- envoi réel par Exchange Online (Graph) ;
- archivage SharePoint ;
- étiquettes QR et campagnes d'inventaire ;
- véhicules ;
- licences logicielles ;
- mutation (M05) et démission (M06).

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
