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

`deploy/backup.sh` produit un dump PostgreSQL, une archive des documents et leurs empreintes SHA-256. Planifiez-le chaque jour (cron root), puis copiez le résultat **hors de la machine**, chiffré (par exemple `gpg --symmetric` puis `rsync` vers le serveur de sauvegarde). Un disque unique n'est pas une sauvegarde.

Restauration sur une autre machine (environnement isolé, sans envoi de mails) :

```bash
sudo docker compose up -d db
sudo docker compose exec -T db pg_restore -U cellulo -d cellulo --clean --if-exists < cellulo-AAAAMMJJ-HHMMSS.dump
sudo docker run --rm -v celluloplast_documents:/data -v "$PWD":/backup debian:trixie-slim \
  sh -c 'tar xzf /backup/documents-AAAAMMJJ-HHMMSS.tar.gz -C /data && chown -R 10001 /data'
sudo docker compose up -d
```

Contrôlez ensuite les références, les rôles, les empreintes des documents et la file des avis en attente avant toute remise en service.

## Démonstration (données fictives uniquement)

Sur une instance de test **vide** :

```bash
sudo docker compose exec -e DEMO_PASSWORD='MotDePasseDemo-2026' app celluloplast seed-demo
```

Cette commande crée deux sites, un compte par rôle (dont un IT + FORMATION), du matériel, une SIM et un catalogue de formations. Elle refuse de s'exécuter si des salariés existent déjà.

## État de cette version (tranches 1 et 2)

Livré : comptes et rôles, annonce d'arrivée, besoins et révisions, file « Mon travail » avec prise atomique, formations, comptes IT et indicateur M365, parc IT (actifs, accessoires, SIM), remises et restitutions partielles avec décharge PDF et dépôt du scan signé, demandes ponctuelles, clôture contrôlée, avis en **mode journal**.

Pas encore livré :
- envoi réel par Exchange Online (Graph) ;
- archivage SharePoint ;
- étiquettes QR et campagnes d'inventaire ;
- véhicules ;
- licences logicielles ;
- import et export Excel ;
- mutation (M05) et démission (M06).
