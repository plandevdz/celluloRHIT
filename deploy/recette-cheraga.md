# Recette terrain à Chéraga : installation (VM A) et restauration (VM B)

Destinataires : équipe IT de Celluloplast.

Version installée : commit `ee76d2dc110036024a885ead803a4866913184b4` (branche `lot3-complet`, PR n° 2 en brouillon). Cette fiche a été ajoutée après ce commit ; elle ne modifie pas le code installé.

**Règles**
- Deux machines de test, distinctes l'une de l'autre et du serveur de production. Aucune commande de cette fiche ne s'exécute sur le serveur de production, et son adresse n'est jamais utilisée.
- Aucun mail (`MAIL_MODE=journal`), aucun SharePoint (`ARCHIVE_MODE=desactive`), fonctions du pilote seulement (`MODULES_DESACTIVES=pilote`).
- Données fictives seulement : jeu `seed-demo` (`@exemple.test`). Aucun fichier du personnel. Les décharges de test sont signées « TEST ».
- Rien de ces machines ne va dans Git : `.env`, sauvegardes, clés, scans, fiche remplie (elle contient des adresses internes).
- Chaque étape porte l'état **À exécuter sur site**. Remplacez-le par la date et vos initiales une fois l'étape faite.

| Machine | Rôle | Durée de vie |
|---|---|---|
| **VM A** | Recette accessible aux utilisateurs de Chéraga (RH, IT, Formation, Graphiste) | Toute la recette terrain |
| **VM B** | Cible vierge de restauration : reçoit la sauvegarde de la VM A | Quelques heures, détruite après |
| Poste d'administration | Détient la clé privée de la sauvegarde de recette ; déchiffre vers la VM B | — |
| PC Windows de Chéraga | Contrôles HTTPS et navigateur | — |

**Prérequis communs aux deux VM**
- Debian 13 préparée selon les étapes 1 à 10 du `README.md` principal : SSH par clé, `ufw`, Docker Engine avec le plugin compose, IP réservée.
- Un compte nominatif dont `sudo` demande un mot de passe.
- Paquets : `sudo apt install -y git openssl gnupg`.
- Accès Internet sortant pendant la construction de l'image (Docker Hub, crates.io, npm). La première construction dure 10 à 20 minutes.

---

## Partie 1 — VM A : recette accessible aux utilisateurs

### A1. Récupérer la version figée

État : **À exécuter sur site**

```bash
sudo mkdir -p /opt/celluloplast && sudo chown "$USER" /opt/celluloplast
git clone https://github.com/plandevdz/celluloRHIT.git /opt/celluloplast
cd /opt/celluloplast
git checkout --detach ee76d2dc110036024a885ead803a4866913184b4
git rev-parse HEAD    # attendu : ee76d2dc110036024a885ead803a4866913184b4
```

### A2. Fichier `.env` à partir de `.env.example`

État : **À exécuter sur site**

```bash
cp .env.example .env && chmod 600 .env
sed -i "s/^POSTGRES_PASSWORD=.*/POSTGRES_PASSWORD=$(openssl rand -hex 24)/" .env
nano .env
```

| Variable | Valeur sur la VM A |
|---|---|
| `SERVER_ADDRESS` | IP réservée de la VM A, ex. `<IP-VM-A>`. Le certificat HTTPS est émis pour cette adresse |
| `TLS_DEFAULT_SNI` | Laisser commenté (une seule adresse) |
| `PUBLIC_URL` | `https://<IP-VM-A>`, même adresse, sans `/` final |
| `POSTGRES_PASSWORD` | Celui que la commande `sed` vient de générer : propre à cette recette, jamais celui de la production |
| `MODULES_DESACTIVES` | `pilote` |
| `MAIL_MODE` | `journal` |
| `ARCHIVE_MODE` | `desactive` |
| `MAIL_TEST_RECIPIENT`, `MAIL_SENDER`, `MAIL_ALLOWED_DOMAINS`, `GRAPH_TENANT_ID`, `GRAPH_CLIENT_ID`, `ARCHIVE_TENANT_ID`, `ARCHIVE_CLIENT_ID`, `ARCHIVE_SITE_ID`, `ARCHIVE_DRIVE_ID` | Vides |
| `SESSION_*`, `ALERT_*`, `ESCALATION_MINUTES`, `ARCHIVE_ROOT` | Valeurs du modèle |

Ne créez pas de dossier `secrets/` : aucun certificat Microsoft n'est utilisé.

Contrôle :

```bash
sudo docker compose config | grep -E 'SERVER_ADDRESS|PUBLIC_URL|MODULES_DESACTIVES|MAIL_MODE|ARCHIVE_MODE'
```

Attendu : `SERVER_ADDRESS: <IP-VM-A>`, `PUBLIC_URL: https://<IP-VM-A>`, `MODULES_DESACTIVES: pilote`, `MAIL_MODE: journal`, `ARCHIVE_MODE: desactive`.

### A3. Construire et démarrer

État : **À exécuter sur site**

```bash
sudo docker compose up -d --build
sudo docker compose ps                              # db (healthy), app, worker, proxy : en marche
sudo docker compose logs app | grep "API démarrée"
```

### A4. Charger les données fictives

État : **À exécuter sur site**

Mot de passe de démonstration : 10 caractères au moins. La commande ci-dessous le demande sans l'afficher ni l'écrire dans l'historique. Communiquez-le aux testeurs de vive voix ou par un canal interne, jamais dans le dépôt.

```bash
read -rs -p "Mot de passe de démonstration : " DEMO_PASSWORD; echo
sudo docker compose exec -e DEMO_PASSWORD="$DEMO_PASSWORD" app celluloplast seed-demo
unset DEMO_PASSWORD
```

Comptes créés, tous avec ce mot de passe :

| Rôle | Compte |
|---|---|
| ADMIN | `admin.demo@exemple.test` |
| RH | `rh.demo@exemple.test` |
| IT | `it.demo@exemple.test`, `it2.demo@exemple.test` |
| IT + Formation | `it.formation.demo@exemple.test` |
| Formation | `formation.demo@exemple.test` |
| Graphiste | `graphiste.demo@exemple.test` |

Le jeu contient les sites « Alger — Siège » et « Oran — Usine ». Pour tester avec le site « Chéraga », l'ADMIN le crée dans Administration → Sites et locaux.

### A5. Certificat racine de la VM A

État : **À exécuter sur site**

```bash
sudo docker compose cp proxy:/data/caddy/pki/authorities/local/root.crt /tmp/recette-a-root.crt
sudo chmod 644 /tmp/recette-a-root.crt
openssl x509 -noout -subject -fingerprint -sha256 -in /tmp/recette-a-root.crt
```

Notez l'**empreinte SHA-256** (la valeur après `=`). Elle sert à l'étape B4.

Copiez `recette-a-root.crt` sur le poste IT (`scp <compte>@<IP-VM-A>:/tmp/recette-a-root.crt .`), puis installez-le **sur les seuls PC de test**, en PowerShell administrateur :

```powershell
certutil -addstore -f Root recette-a-root.crt
```

Après la recette, retirez-le : `certmgr.msc` → Autorités de certification racines de confiance → « Caddy Local Authority… ».

### A6. Pare-feu

État : **À exécuter sur site**

Docker contourne `ufw` pour les ports publiés. Limitez 80 et 443 aux PC de Chéraga :

```bash
sudo iptables -I DOCKER-USER -p tcp -m multiport --dports 80,443 ! -s <sous-réseau-des-PC-de-Chéraga> -j DROP
```

Sans le paquet `iptables-persistent`, la règle disparaît au redémarrage de la VM.

### A7. Contrôle depuis un PC Windows de Chéraga

État : **À exécuter sur site**

PowerShell, sans droits d'administrateur :

```powershell
$vmA = '<IP-VM-A>'
Test-NetConnection $vmA -Port 443                       # TcpTestSucceeded : True
curl.exe --ssl-no-revoke -sS "https://$vmA/api/v1/health"
```

Attendu, sans erreur de certificat :
- `"status":"ok"` ;
- `"mail_mode":"journal"` ;
- `"archive_mode":"desactive"` ;
- `"modules_desactives"` avec les six modules : `vehicules`, `logiciels`, `inventaire`, `mutation`, `depart`, `etats`.

`--ssl-no-revoke` désactive seulement le contrôle de révocation, que l'autorité interne ne publie pas. Le certificat reste vérifié. Une erreur `SEC_E_UNTRUSTED_ROOT` signifie que le certificat de l'étape A5 n'est pas installé sur ce PC.

Puis dans Edge ou Chrome, à l'adresse `https://<IP-VM-A>` :

| # | Contrôle | Attendu |
|---|---|---|
| 1 | Ouvrir la page | Cadenas, aucun avertissement, page de connexion |
| 2 | Se connecter avec `rh.demo@exemple.test` ; regarder le menu et l'accueil | Menu : Accueil RH, Mon travail, Dossiers, Personnel, Formations. Ni Véhicules ni États. Aucune section départs ou mutations sur l'accueil |
| 3 | Personnel → ouvrir une fiche salarié | Aucun bouton « Mutation », « Départ » ni « Correction de poste datée » |

La recette fonctionnelle se poursuit avec [docs/recette-pilote.md](../docs/recette-pilote.md), partie 1 : parcours papier, douchette, classeur Excel, demandes sans mail.

### A8. Préparer le jeu à restaurer

État : **À exécuter sur site**

Avant la sauvegarde, il faut au moins une remise dont la décharge a été imprimée, signée « TEST », scannée et déposée. Sur le dossier, la remise affiche « Signé (scan déposé) ».

Relevez :
- la référence du dossier (`ONB-…`) et celle du mouvement (`MVT-…`) ;
- les empreintes enregistrées en base :

```bash
cd /opt/celluloplast
sudo docker compose exec -T db psql -U cellulo -d cellulo -c "select m.reference, v.file_kind, v.version, v.sha256 from document_versions v join documents d on d.id = v.document_id join movements m on m.id = d.movement_id order by v.created_at"
sudo docker compose exec -T db psql -U cellulo -d cellulo -c "select count(*) as dossiers from cases"
```

---

## Partie 2 — Sauvegarde chiffrée de la VM A

### B1. Clé de sauvegarde de recette (poste d'administration)

État : **À exécuter sur site**

Utilisez une clé dédiée à la recette, distincte de la future clé de production. Sur le poste d'administration : Linux, WSL ou Git Bash, qui fournissent `gpg`, `ssh`, `scp` et `sha256sum`. Utilisez le même outil pour toutes les étapes du poste, afin de garder un seul trousseau.

```bash
gpg --quick-generate-key "Celluloplast sauvegarde RECETTE" rsa4096 encr 6m      # phrase de passe forte
FPR=$(gpg --list-keys --with-colons "Celluloplast sauvegarde RECETTE" | awk -F: '$1=="fpr"{print $10; exit}')
echo "$FPR"                                                                    # empreinte de la clé
gpg --armor --export "$FPR" > recette-pubkey.asc                               # partie PUBLIQUE seulement
scp recette-pubkey.asc <compte>@<IP-VM-A>:
```

La clé privée ne quitte pas le poste d'administration.

### B2. Sauvegarder sur la VM A

État : **À exécuter sur site**

```bash
sudo install -d -m 755 /etc/celluloplast
sudo install -m 644 ~/recette-pubkey.asc /etc/celluloplast/backup-pubkey.asc
echo 'BACKUP_GPG_RECIPIENT=<FPR>' | sudo tee /etc/celluloplast/backup.conf >/dev/null
sudo chmod 600 /etc/celluloplast/backup.conf
cd /opt/celluloplast
sudo deploy/backup.sh "$HOME/sauvegarde-recette"; echo "code=$?"
sudo chown -R "$USER" "$HOME/sauvegarde-recette"
```

Attendu : « Sauvegarde COMPLÈTE, chiffrée pour la clé … » et `code=0`. Quatre fichiers sont produits :
- `cellulo-<D>.dump.gpg` ;
- `documents-<D>.tar.gz.gpg` ;
- `secrets-<D>.tar.gz.gpg`, qui contient le `.env` et l'autorité de certification de la VM A ;
- `sha256-<D>.txt`.

`<D>` est l'horodatage `AAAAMMJJ-HHMMSS`.

Si `code=3` (« SAUVEGARDE INCOMPLÈTE »), arrêtez-vous : le chiffrement n'est pas configuré.

### B3. Rapatrier sur le poste d'administration

État : **À exécuter sur site**

```bash
scp -r <compte>@<IP-VM-A>:sauvegarde-recette ./sauvegarde-recette
cd sauvegarde-recette && sha256sum -c sha256-*.txt     # attendu : OK pour chaque fichier
```

---

## Partie 3 — VM B : restauration sur une cible vierge

La VM B reçoit le jeu fictif de la VM A, **avec l'autorité de certification de la VM A** (celle de la recette, jamais celle de la production). Un PC qui fait déjà confiance à la VM A ouvre donc la VM B sans rien réinstaller. Le script vérifie que Caddy n'en génère pas une nouvelle.

Dans les messages de `restore-isole.sh`, le mot « PRODUCTION » désigne ici la source de la sauvegarde, c'est-à-dire la VM A.

### B4. Préparer la VM B

État : **À exécuter sur site**

La VM B doit avoir une IP différente de celle de la VM A : le script refuse l'adresse de la machine sauvegardée. Clonez le dépôt au même commit, sans rien démarrer : pas de `docker compose up`, pas de `seed-demo`, pas de `.env`. Le script crée son propre environnement, avec un mot de passe PostgreSQL neuf.

```bash
sudo mkdir -p /opt/celluloplast && sudo chown "$USER" /opt/celluloplast
git clone https://github.com/plandevdz/celluloRHIT.git /opt/celluloplast
cd /opt/celluloplast
git checkout --detach ee76d2dc110036024a885ead803a4866913184b4
git rev-parse HEAD                         # attendu : ee76d2dc110036024a885ead803a4866913184b4
sudo docker ps -a; sudo docker volume ls   # attendu : aucun conteneur, aucun volume
```

### B5. Déchiffrer vers la VM B (poste d'administration)

État : **À exécuter sur site**

Ouvrez un terminal dédié : `set -euo pipefail` le ferme à la première erreur. La première commande `gpg` demande la phrase de passe et la garde en cache pour les suivantes.

```bash
set -euo pipefail
H=<compte>@<IP-VM-B>; D=<AAAAMMJJ-HHMMSS>
cd sauvegarde-recette && sha256sum -c sha256-$D.txt
gpg -d cellulo-$D.dump.gpg > /dev/null
ssh "$H" 'umask 077 && mkdir -p ~/restau'
gpg -d cellulo-$D.dump.gpg       | ssh "$H" 'umask 077 && cat > ~/restau/cellulo.dump'
gpg -d documents-$D.tar.gz.gpg   | ssh "$H" 'umask 077 && cat > ~/restau/documents.tar.gz'
gpg -d secrets-$D.tar.gz.gpg     | ssh "$H" 'umask 077 && cat > ~/restau/secrets.tar.gz'
```

### B6. Restaurer (VM B, session interactive `ssh -t`)

État : **À exécuter sur site**

```bash
cd /opt/celluloplast
sudo deploy/restore-isole.sh --source ~/restau --adresse <IP-VM-B> \
     --machine-de-test-confirmee --empreinte-ca-attendue <empreinte-SHA-256-relevée-en-A5> \
     --supprimer-source
```

Attendu en fin de script :
- `empreintes PDF/scans : N/N conformes` ;
- `certificat racine Caddy : identique avant/après` ;
- `mode d'envoi : journal` ;
- `RECETTE DE RESTAURATION RÉUSSIE (projet cellulo-restore)`.

En cas de « RECETTE EN ÉCHEC » ou d'« ARRÊT » : les fichiers sont conservés. Transmettez la sortie du script, sans le contenu de `~/restau`. Avant un nouvel essai : `sudo docker compose -p cellulo-restore --env-file /root/cellulo-restore.env down -v`.

Ouvrez ensuite 443 au seul PC IT de Chéraga :

```bash
sudo iptables -I DOCKER-USER -p tcp -m multiport --dports 80,443 ! -s <IP-du-PC-IT> -j DROP
```

### B7. Résultat attendu : l'IT rouvre la même décharge et le même scan

État : **À exécuter sur site**

Depuis le PC IT de Chéraga, qui a le certificat de la VM A et **aucun nouveau certificat** :

| # | Contrôle | Attendu |
|---|---|---|
| 1 | `curl.exe --ssl-no-revoke -sS https://<IP-VM-B>/api/v1/health` | `"status":"ok"`, `journal`, `desactive`, six modules fermés, aucune erreur de certificat |
| 2 | Edge → `https://<IP-VM-B>` | Cadenas, aucun avertissement |
| 3 | Connexion `it.demo@exemple.test`, avec le même mot de passe de démonstration que sur la VM A | Connexion acceptée |
| 4 | Dossiers → le dossier `ONB-…` relevé en A8 → « Remis et restitué » | Le mouvement `MVT-…` est présent, avec « Signé (scan déposé) » |
| 5 | « Imprimer la décharge (PDF) » | Même décharge : même référence `MVT-…`, mêmes biens, même salarié |
| 6 | Clic droit sur « Imprimer la décharge (PDF) » → Copier le lien. Collez-le dans la barre d'adresse en remplaçant `original` par `scan/1` (ou le numéro de version relevé en A8) | Le scan signé « TEST » s'ouvre (`MVT-…-scan-1`) |
| 7 | Sur la VM B, la requête d'empreintes de l'étape A8, précédée de `sudo docker compose -p cellulo-restore --env-file /root/cellulo-restore.env` à la place de `sudo docker compose` | Mêmes valeurs `sha256` qu'en A8. Même nombre de dossiers |

L'interface de cette version n'a pas de bouton « Ouvrir le scan » : le contrôle 6 passe par l'adresse directe, réservée aux RH, ADMIN et IT.

### B8. Démonter la VM B

État : **À exécuter sur site**

```bash
cd /opt/celluloplast
sudo docker compose -p cellulo-restore --env-file /root/cellulo-restore.env down -v
sudo shred -u /root/cellulo-restore.env
```

Retirez ensuite la règle de pare-feu et détruisez la VM.

Sur le poste d'administration, gardez `sauvegarde-recette` et la clé de recette jusqu'à la fin de la recette terrain, puis supprimez-les :

```bash
gpg --delete-secret-and-public-key <FPR>
```

---

## Résultats à reporter

À remplir sur papier ou dans un document interne, pas dans Git :

| Point | Résultat |
|---|---|
| A7 : HTTPS depuis un PC de Chéraga, sans avertissement | |
| A7 : RH sans Mutation ni Départ | |
| B2 : sauvegarde complète, `code=0` | |
| B6 : restauration réussie (PDF/scans N/N, CA identique) | |
| B7 : même décharge et même scan rouverts par l'IT | |
| Écarts constatés | |
