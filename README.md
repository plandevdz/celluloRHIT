# Celluloplast | Parcours salarié
Dossier de cadrage v2.0 • 12 novembre 2026 • Destinataires : DSI, RH, développeur et référent recette.

**Livrable : spécifications et code source de l’application.** Le projet remplace Power Apps et Power Automate par une **application web** (navigateur, aucun logiciel à installer sur les postes) servie par un service Rust central, hébergé en conteneurs Docker sur un serveur Debian de l’entreprise et joint par son adresse IP. PostgreSQL devient la source de vérité. Exchange Online envoie les notifications ; SharePoint reste la destination des décharges et scans, conformément au besoin documentaire initial. Pas de double saisie durable entre anciennes listes et nouvelle base.

## Avant le développement : préparer la machine
1. Relever le modèle du processeur, la RAM et l’état SMART du disque ; tester la mémoire, l’Ethernet et le redémarrage après coupure secteur. Sauvegarder ce qui existe avant tout partitionnement. Proposition pour pilote, à mesurer : CPU x86-64, 4 cœurs, 8 Go de RAM, SSD sain de 128 Go ou plus et sauvegarde sur autre machine. Un disque unique n’est pas une sauvegarde. Prévoir onduleur et machine de reprise avant production.
2. Télécharger la netinst Debian 13 stable amd64 depuis https://www.debian.org/distrib/netinst et vérifier la somme SHA-256 contre le manifeste signé Debian. amd64 convient aux PC Intel et AMD 64 bits ; ne pas utiliser cette image pour un CPU uniquement 32 bits. Démarrer sur clé USB, installer sur le disque identifié. Choisir Afrique/Alger, créer un utilisateur nominatif administrateur (exemple : youcef), pas de connexion SSH root. Laisser le mot de passe root vide dans l’installateur Debian pour confier sudo au premier utilisateur ; sinon configurer sudo depuis la console root avant de continuer.
3. Nom proposé : srv-parcours. Cocher « serveur SSH » et « utilitaires usuels du système » ; décocher l’environnement de bureau et tous les bureaux graphiques. Brancher en Ethernet. Dans le DHCP du site, réserver une IP disponible à l’adresse MAC du serveur. Créer si possible un nom DNS interne. Les exemples ci-dessous utilisent 10.0.0.50 pour le serveur et 10.0.0.25 pour le poste IT : **remplacer les deux** ; ce ne sont pas des adresses attribuées.
4. À la console Debian :
   ```bash
   ip -br addr
   ip route
   sudo apt update
   sudo apt upgrade
   sudo apt install openssh-server ufw
   sudo systemctl enable --now ssh
   sudo systemctl status ssh --no-pager
   sudo ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
   ```
   Noter l’empreinte SHA256 affichée. Si une mise à jour exige un redémarrage, le faire à la console puis vérifier le service et l’IP.
5. Sur un serveur dédié neuf, limiter SSH au poste IT (IP réservée) ou au sous-réseau d’administration défini par la DSI. Depuis la **console locale**, après revue des règles déjà présentes :
   ```bash
   sudo ufw status verbose
   sudo ufw default deny incoming
   sudo ufw default allow outgoing
   sudo ufw allow from 10.0.0.25 to any port 22 proto tcp
   sudo ufw enable
   sudo ufw status numbered
   ```
   Vérifier qu’il n’existe aucune autre autorisation large sur 22, IPv6 compris. Ne pas rediriger le port 22 sur Internet. Le filtrage local compte même si deux PC sont sur le même switch. Pour un autre site, autoriser exclusivement les sources IT routées via le VPN d’entreprise. Ne pas ouvrir PostgreSQL aux clients.
6. Depuis **PowerShell Windows**, client OpenSSH installé :
   ```powershell
   $serveur = '10.0.0.50'
   $admin = 'youcef'
   $cle = Join-Path $env:USERPROFILE '.ssh/celluloplast_parcours'
   Test-NetConnection $serveur -Port 22
   ssh -o HostKeyAlgorithms=ssh-ed25519 "$admin@$serveur"
   ```
   Comparer la première empreinte à celle de la console avant d’accepter. Entrer le mot de passe de l’utilisateur Debian, tester `sudo -v`, puis `exit`. Si `ssh` manque, installer la fonctionnalité facultative Windows **Client OpenSSH**. Si le port est inaccessible, vérifier adresse source, réservation DHCP, service et pare-feu avant toute modification.
7. Toujours dans PowerShell, créer une clé dédiée **si ce fichier n’existe pas** ; protéger la clé privée par une phrase de passe. Copier uniquement la partie publique :
   ```powershell
   ssh-keygen -t ed25519 -f $cle -C 'admin-poste-IT-parcours'
   Get-Content "$cle.pub" | ssh "$admin@$serveur" 'umask 077; mkdir -p ~/.ssh; tr -d "\r" >> ~/.ssh/authorized_keys; chmod 700 ~/.ssh; chmod 600 ~/.ssh/authorized_keys'
   ssh -i $cle -o IdentitiesOnly=yes -o PreferredAuthentications=publickey "$admin@$serveur"
   ```
   Tester `sudo -v` et **garder cette session ouverte**. Ajouter une clé nominative par administrateur ; pas de partage de clé privée.
8. Sur Debian, après succès du test par clé :
   ```bash
   sudo tee /etc/ssh/sshd_config.d/00-celluloplast.conf >/dev/null <<'SSHCONF'
   PermitRootLogin no
   PubkeyAuthentication yes
   PasswordAuthentication no
   KbdInteractiveAuthentication no
   SSHCONF
   sudo /usr/sbin/sshd -t && sudo systemctl reload ssh
   sudo /usr/sbin/sshd -T | grep -E '^(permitrootlogin|pubkeyauthentication|passwordauthentication|kbdinteractiveauthentication) '
   ```
   Depuis une **deuxième fenêtre** PowerShell, reprendre les variables de l’étape 6 et refaire le test par clé. Ne fermer la première session qu’après succès. En cas d’erreur, reprendre la console, corriger ou déplacer le fichier de configuration, tester avec sshd -t et recharger ssh. Conserver l’accès physique. Les utilisateurs de l’application ne reçoivent aucun compte SSH.
9. Installer Docker Engine et le plugin compose depuis le dépôt officiel Docker pour Debian (https://docs.docker.com/engine/install/debian/), puis `sudo systemctl enable --now docker` et `sudo docker run --rm hello-world`. Ne pas ajouter les utilisateurs de l’application au groupe `docker` (équivaut à root).
10. Autoriser HTTPS/443 (et 80, qui redirige vers 443) depuis les sous-réseaux utilisateurs confirmés uniquement : `sudo ufw allow from <sous-réseau> to any port 443 proto tcp`. **Attention** : les ports publiés par Docker contournent ufw ; le fichier compose ne publie que 80/443 du proxy, et le filtrage par sous-réseau doit être posé dans la chaîne `DOCKER-USER` ou sur le pare-feu réseau. Ne jamais publier PostgreSQL. Le certificat racine de la CA interne Caddy est installé sur les PC Windows (voir deploy/README.md). Le serveur doit pouvoir joindre les services Microsoft nécessaires en HTTPS sortant ; SSH local ne nécessite aucun accès Internet entrant.

## Dossier de développement v2
Lire dans l’ordre : **01-Metier.md**, **03-UI-UX.md**, **04-Donnees.md**, **02-Technique.md**, puis **05-Recette.md**. Le dossier modeles/ contient deux modèles A4 HTML ouvrables et imprimables et un classeur Excel vierge. Le rendu PDF définitif est produit par le serveur depuis les mouvements réels. Le code source est versionné dans https://github.com/plandevdz/celluloRHIT ; l’installation est décrite dans deploy/README.md.

Par rapport à la v1 : cinq rôles cumulables, cinq champs pour annoncer une arrivée, avis immédiat aux équipes, préférences RH ajustables par l’IT, demandes ponctuelles, stocks et logiciels, véhicules RH, QR, Excel et retours partiels. Le salarié n’a pas besoin d’un compte Microsoft. Les règles des processus M05 et M06 constituent les choix retenus pour le développement.

### Ordre des livraisons pour Claude Code
1. Figer les contrats et migrations ; créer environnement isolé (docker compose), comptes multi-rôles, références publiques, sauvegarde/restauration et interface web servie en HTTPS.
2. Tranche verticale : arrivée avec cinq champs → avis IT/Formation en mode test → prise de tâche → préférence fixe remplacée par portable → remise avec décharge. Valider R01 à R05.
3. Terminer parc IT, restitutions, QR, licences, parcs véhicules RH, demandes ponctuelles et Excel. Valider les écarts et les reprises après panne.
4. Mutation, démission et traitements à échéance ; valider les dates, les conflits de poste et la séparation sortie/coupure/retour.
5. Piloter sur un site avec données fictives, tester imprimante et douchette réelles ; reprise SharePoint à blanc, rapprochement, sauvegarde restaurée et bascule maîtrisée.

### Avancement (12/11/2026)
| Étape | Implémenté | Testé automatiquement | Vérifié dans le navigateur | À valider sur site |
|---|---|---|---|---|
| 1. Socle (rôles, références, HTTPS, Docker, sauvegarde) | oui | oui | oui | restauration Docker complète, CA conservée |
| 2. Tranche verticale arrivée → remise (R01–R06) | oui | oui | oui | impression papier, douchette, e-mails réels |
| Lot 1 : périmètre des données, présence réelle, preuves de fin de tâche, avis exacts | oui | oui | oui | — |
| 3. Excel du personnel (R08) | oui | oui | oui | classeur RH réel |
| 3. QR et inventaire (R07), licences, véhicules (R11, R12), reprise Exchange/SharePoint (R13) | non | — | — | — |
| 4. Mutation, démission (R09, R10) | non | — | — | — |
| 5. Pilote | non | — | — | — |

Le détail par scénario est dans 05-Recette.md. Prochaine tranche prévue : envoi Exchange vers une boîte de test avec reprise sur erreur, puis étiquettes QR et campagnes d’inventaire.

Pour chaque tranche : livrer code, migrations, tests d’intégration, données fictives, démonstration dans un navigateur sur poste Windows et écarts restants. Ne pas activer les anciens flux ni envoyer de vrais mails pendant les tests. Ne pas intégrer de secrets dans le projet. Avant production seulement : confirmer matériel, IP/DNS, expéditeur Exchange, bibliothèque documentaire, cibles de sauvegarde et destinataires d’escalade.

Sources infrastructure : https://www.debian.org/releases/stable/ • https://man.openbsd.org/sshd_config • https://learn.microsoft.com/windows-server/administration/openssh/openssh_keymanagement
