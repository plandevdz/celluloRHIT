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
| Base PostgreSQL : dossiers, rôles, audit, références, empreintes des documents | `cellulo-<date>.dump` | `pg_dump` en format custom |
| Décharges PDF originales, scans signés | `documents-<date>.tar.gz` | volume `celluloplast_documents` |
| Configuration `.env` (mot de passe PostgreSQL, adresses) | `secrets-<date>.tar.gz` | **sensible** |
| Autorité de certification interne de Caddy (certificat racine et **clé privée**) | `secrets-<date>.tar.gz` | **sensible**. Sans elle, une restauration crée une nouvelle autorité et tous les PC Windows doivent réinstaller le certificat racine. |
| Empreintes SHA-256 des fichiers ci-dessus | `sha256-<date>.txt` | à vérifier avant restauration |
| Code et image Docker | — | reconstruits depuis Git (`docker compose up -d --build`) |

`sudo deploy/backup.sh /var/backups/celluloplast` crée ces fichiers avec des droits 600. Utilisez `--sans-secrets` pour exclure `.env` et la CA.

Planifiez la sauvegarde chaque jour (cron root), puis copiez-la **hors de la machine**. Chiffrez au minimum l'archive des secrets avant toute copie :

```bash
gpg --symmetric --cipher-algo AES256 /var/backups/celluloplast/secrets-AAAAMMJJ-HHMMSS.tar.gz
shred -u /var/backups/celluloplast/secrets-AAAAMMJJ-HHMMSS.tar.gz
rsync -a /var/backups/celluloplast/ sauvegarde@serveur-de-sauvegarde:/celluloplast/
```

La phrase de passe GPG se conserve hors du serveur, par exemple dans le coffre de la DSI. Un disque unique n'est pas une sauvegarde. Aucun de ces fichiers ne doit être déposé dans Git.

Pour remplacer plutôt la CA interne par un certificat de la PKI d'entreprise, mettez `tls /certs/cert.pem /certs/key.pem` dans `deploy/Caddyfile` et montez ces fichiers en lecture seule dans le service `proxy`. Les PC qui font confiance à la PKI d'entreprise n'ont alors rien à installer.

### Restauration (instance isolée, volumes neufs)

Sur une autre machine, ou sous un autre nom de projet pour ne pas toucher la production. L'envoi de mails reste en mode journal.

```bash
export COMPOSE_PROJECT_NAME=cellulo-restore
B=/chemin/des/sauvegardes; D=AAAAMMJJ-HHMMSS
cd "$B" && sha256sum -c sha256-$D.txt                                    # 1. intégrité
gpg -d secrets-$D.tar.gz.gpg > /tmp/secrets.tar.gz                      # 2. secrets (si chiffrés)
cd /opt/celluloplast
tar xzf /tmp/secrets.tar.gz -C /tmp && cp /tmp/cfg/.env .env && chmod 600 .env
sudo docker compose up -d db                                             # 3. base
sudo docker compose exec -T db pg_restore -U cellulo -d cellulo --clean --if-exists < "$B/cellulo-$D.dump"
sudo docker run --rm -v ${COMPOSE_PROJECT_NAME}_documents:/data -v "$B":/backup debian:trixie-slim \
  sh -c "tar xzf /backup/documents-$D.tar.gz -C /data && chown -R 10001 /data"   # 4. documents
sudo docker run --rm -v ${COMPOSE_PROJECT_NAME}_caddy_data:/caddy -v /tmp/caddy:/src debian:trixie-slim \
  sh -c "mkdir -p /caddy/caddy && cp -a /src/caddy/pki /caddy/caddy/"            # 5. même CA (même certificat racine)
shred -u /tmp/secrets.tar.gz; rm -rf /tmp/cfg /tmp/caddy
sudo docker compose up -d --build                                        # 6. application
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
- restauration base + documents : **exécutée** sur une instance PostgreSQL isolée, hors Docker (voir 05-Recette.md, « Reprise ») ;
- `backup.sh` et la restauration par volumes Docker ci-dessus : **non exécutés**, faute de Docker utilisable sur le poste de développement. À jouer sur une machine de test avant la mise en production.

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
