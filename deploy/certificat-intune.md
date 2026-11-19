# Déployer le certificat HTTPS sur les PC avec Intune

Ce guide installe le **certificat racine** du serveur « Celluloplast | Parcours salarié » sur tous les PC Windows gérés par Intune, au moyen d'un script PowerShell. Une fois ce certificat installé, `https://<IP-du-serveur>/` s'ouvre dans Edge, Chrome et Firefox sans avertissement.

Durée : 15 minutes de préparation, puis quelques heures pour que tous les PC reçoivent le script (cycle normal d'Intune).

## Sommaire

1. [Pourquoi ce certificat](#1-pourquoi-ce-certificat)
2. [Ce dont vous avez besoin](#2-ce-dont-vous-avez-besoin)
3. [Étape 1 : récupérer le certificat racine du serveur](#étape-1--récupérer-le-certificat-racine-du-serveur)
4. [Étape 2 : générer les scripts](#étape-2--générer-les-scripts)
5. [Étape 3 : essayer sur un seul PC, à la main](#étape-3--essayer-sur-un-seul-pc-à-la-main)
6. [Étape 4 : créer les groupes de PC dans Entra ID](#étape-4--créer-les-groupes-de-pc-dans-entra-id)
7. [Étape 5 : déployer le script de plateforme dans Intune](#étape-5--déployer-le-script-de-plateforme-dans-intune)
8. [Étape 6 (facultative) : correction automatique](#étape-6-facultative--correction-automatique)
9. [Étape 7 : suivre le déploiement](#étape-7--suivre-le-déploiement)
10. [Dépannage](#dépannage)
11. [Autre méthode sans script : le profil « Certificat approuvé »](#autre-méthode-sans-script--le-profil--certificat-approuvé-)
12. [Changement d'autorité, retrait](#changement-dautorité-retrait)
13. [Règles de sécurité](#règles-de-sécurité)

---

## 1. Pourquoi ce certificat

Le serveur est joint par son adresse IP. Caddy, le serveur HTTPS de l'application, délivre lui-même le certificat du site à partir de sa propre **autorité de certification interne**, « Caddy Local Authority ». Cette autorité :

| Élément | Durée | Ce qu'il faut faire |
|---|---|---|
| Certificat **racine** (« … ECC Root ») | 10 ans | **L'installer une fois** sur chaque PC : c'est l'objet de ce guide |
| Certificat intermédiaire | 7 jours, renouvelé automatiquement | Rien |
| Certificat du site | quelques heures, renouvelé automatiquement | Rien |

Les PC ne connaissent pas cette autorité. Sans le certificat racine, le navigateur affiche « Votre connexion n'est pas privée ». Ne demandez jamais aux utilisateurs d'ignorer cet avertissement : ils prendraient l'habitude de cliquer sur « Continuer » partout.

Le script ajoute le certificat dans **Ordinateur local → Autorités de certification racines de confiance**. Il vaut alors pour tous les comptes du PC, ainsi que pour Edge, Chrome et, avec la stratégie ajoutée par le script, Firefox.

## 2. Ce dont vous avez besoin

| Besoin | Détail |
|---|---|
| PC | Windows 10 (1607 ou plus) ou Windows 11, **inscrits dans Intune** (joints à Entra ID, ou en jonction hybride) |
| Rôle Intune | « Administrateur Intune » ou « Gestionnaire de stratégies et de profils » |
| Accès au serveur | SSH avec un compte autorisé à `sudo` (`<compte-ssh>@<IP-du-serveur>`) |
| Poste d'administration | Windows avec PowerShell (5.1 suffit) et Git Bash (ou `scp` de Windows) |
| Ce dépôt | Le générateur `deploy/intune/Generer-ScriptsIntune.ps1` |

Pour savoir si un PC est inscrit : centre d'administration Intune (<https://intune.microsoft.com>) → **Appareils** → **Tous les appareils** → cherchez le nom du PC. Sur le PC lui-même : **Paramètres → Comptes → Accès Professionnel ou Scolaire** doit afficher le compte de l'entreprise avec « Connecté à … Azure AD / Entra ID ».

---

## Étape 1 : récupérer le certificat racine du serveur

**Sur le serveur (SSH) :**

```bash
ssh <compte-ssh>@<IP-du-serveur>
cd /opt/celluloplast
sudo docker compose cp proxy:/data/caddy/pki/authorities/local/root.crt /tmp/celluloplast-root.crt
sudo chmod 644 /tmp/celluloplast-root.crt
openssl x509 -noout -subject -enddate -in /tmp/celluloplast-root.crt
openssl x509 -noout -fingerprint -sha1   -in /tmp/celluloplast-root.crt
openssl x509 -noout -fingerprint -sha256 -in /tmp/celluloplast-root.crt
exit
```

Notez les deux empreintes affichées. Le sujet doit être `CN = Caddy Local Authority - <année> ECC Root`.

> C'est bien `root.crt` qu'il faut, pas `intermediate.crt` : l'intermédiaire change chaque semaine. Le générateur refuse d'ailleurs tout certificat qui n'est pas une racine.

**Sur votre poste (Git Bash) :**

```bash
mkdir -p ~/Desktop/Celluloplast-Intune
scp <compte-ssh>@<IP-du-serveur>:/tmp/celluloplast-root.crt ~/Desktop/Celluloplast-Intune/
ssh <compte-ssh>@<IP-du-serveur> 'rm /tmp/celluloplast-root.crt'
```

Le certificat racine est **public** : il ne contient pas la clé privée de l'autorité, qui reste sur le serveur. Le copier ne présente pas de risque.

## Étape 2 : générer les scripts

Le générateur embarque le certificat dans les scripts, car Intune ne distribue qu'un seul fichier par script. Il vérifie aussi que vous avez exporté la bonne pièce.

**Dans PowerShell, sur votre poste** (depuis la racine du dépôt) :

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\deploy\intune\Generer-ScriptsIntune.ps1 `
  -Certificat "$env:USERPROFILE\Desktop\Celluloplast-Intune\celluloplast-root.crt" `
  -Sortie     "$env:USERPROFILE\Desktop\Celluloplast-Intune"
```

Résultat dans `Bureau\Celluloplast-Intune` :

| Fichier | Rôle |
|---|---|
| `Installer-CertificatCelluloplast.ps1` | **Le script à déployer** : il installe le certificat (et la stratégie Firefox) |
| `Detecter-CertificatCelluloplast.ps1` | Script de détection, pour l'étape 6 facultative |
| `Desinstaller-CertificatCelluloplast.ps1` | Retrait, en cas de changement d'autorité |
| `empreintes.txt` | Les empreintes du certificat embarqué |

**Contrôle obligatoire :** les empreintes SHA-1 et SHA-256 affichées par le générateur (et écrites dans `empreintes.txt`) doivent être **identiques** à celles d'`openssl` à l'étape 1. Windows écrit les empreintes sans les deux-points (`2E6656ED…`), openssl avec (`2E:66:56:ED:…`) : seul le séparateur diffère. Si les valeurs ne correspondent pas, ne déployez rien.

Options du générateur :
- `-SansFirefox` : n'ajoute pas la stratégie Firefox, si Firefox n'est pas utilisé ou s'il est déjà géré autrement.

Ce que fait `Installer-CertificatCelluloplast.ps1`, dans l'ordre :
1. il crée `C:\ProgramData\Celluloplast\` et écrit son journal dans `certificat-racine.log` ;
2. il reconstitue le certificat embarqué et vérifie son empreinte (un script modifié échoue) ;
3. si le certificat est déjà dans `LocalMachine\Root`, il ne fait rien ; sinon il l'ajoute ;
4. il vérifie que le certificat est bien présent après l'ajout ;
5. il active la stratégie Firefox `ImportEnterpriseRoots`, pour que Firefox reprenne les racines de Windows ;
6. il sort avec le code **0** (réussite) ou **1** (échec, motif dans le journal).

Le script peut être relancé autant de fois que nécessaire.

## Étape 3 : essayer sur un seul PC, à la main

Avant Intune, vérifiez le script sur un PC de test qui affiche encore l'avertissement de certificat.

1. Copiez `Installer-CertificatCelluloplast.ps1` sur ce PC (clé USB ou partage).
2. Ouvrez **Windows PowerShell en tant qu'administrateur** (clic droit sur le menu Démarrer → « Terminal (administrateur) »).
3. Lancez :
   ```powershell
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\chemin\vers\Installer-CertificatCelluloplast.ps1"
   echo "code=$LASTEXITCODE"
   ```
   Le résultat attendu est `Ajouté : CN=Caddy Local Authority - … ECC Root (…)`, puis `Terminé avec succès.` et `code=0`.
4. Fermez **toutes** les fenêtres d'Edge ou de Chrome (le navigateur garde le certificat en mémoire), rouvrez-le, puis allez sur `https://<IP-du-serveur>/`. Le cadenas s'affiche, sans avertissement.
5. Relancez le script : il doit afficher `Déjà présent` et `code=0`.

Pour vérifier le magasin : `certlm.msc` → **Autorités de certification racines de confiance → Certificats**, puis cherchez « Caddy Local Authority ». En PowerShell :
```powershell
Get-ChildItem Cert:\LocalMachine\Root | Where-Object Subject -like '*Caddy Local Authority*' | Format-List Subject, Thumbprint, NotAfter
```

## Étape 4 : créer les groupes de PC dans Entra ID

Créez deux groupes : un pilote, puis un groupe pour tous les PC qui doivent ouvrir l'application.

1. <https://intune.microsoft.com> → **Groupes** → **Nouveau groupe**.
2. **Type de groupe** : Sécurité. **Nom** : `Celluloplast - Certificat HTTPS - Pilote`. **Type d'appartenance** : Affecté.
3. **Membres** → ajoutez 2 ou 3 **appareils** (et non des utilisateurs) : le vôtre et celui d'un RH, par exemple.
4. Recommencez pour `Celluloplast - Certificat HTTPS - Tous`. Deux possibilités :
   - un groupe **Affecté**, avec les PC de l'IT, des RH, de la formation et du graphiste ;
   - ou un groupe **Appareil dynamique**, avec une règle comme
     `(device.deviceOSType -eq "Windows") and (device.managementType -eq "MDM")`, ou
     `(device.deviceOSType -eq "Windows") and (device.deviceOwnership -eq "Company")`.

> Visez des **groupes d'appareils** : le certificat est installé pour l'ordinateur (en SYSTEM). Un groupe d'utilisateurs fonctionne aussi (le script part alors sur les PC où ces utilisateurs se connectent), mais il s'exécute plus tard et de façon moins prévisible.

## Étape 5 : déployer le script de plateforme dans Intune

1. <https://intune.microsoft.com> → **Appareils** → **Scripts et corrections** → onglet **Scripts de plateforme** → **+ Ajouter** → **Windows 10 et versions ultérieures**.
2. **Informations de base** :
   - Nom : `Celluloplast - Certificat racine HTTPS`
   - Description : `Installe l'autorité « Caddy Local Authority » du serveur Celluloplast (empreinte <EMPREINTE-SHA1>) dans les racines de confiance de l'ordinateur.`
3. **Paramètres du script** :

   | Paramètre | Valeur |
   |---|---|
   | Emplacement du script | `Installer-CertificatCelluloplast.ps1` |
   | Exécuter ce script à l'aide des informations d'identification de l'utilisateur connecté | **Non** (il s'exécute en SYSTEM : c'est nécessaire pour écrire dans le magasin de l'ordinateur) |
   | Appliquer la vérification de la signature du script | **Non** (le script n'est pas signé) |
   | Exécuter le script dans l'hôte PowerShell 64 bits | **Oui** (sinon les clés de registre Firefox vont dans la branche 32 bits) |

4. **Affectations** → **Groupes inclus** → `Celluloplast - Certificat HTTPS - Pilote`.
5. **Vérifier + créer** → **Ajouter**.

**Quand le script s'exécute-t-il ?** L'extension de gestion Intune (« Intune Management Extension ») s'installe d'elle-même sur les PC dès qu'un script leur est affecté. Elle vérifie les nouveaux scripts au démarrage, à l'ouverture de session et toutes les 8 heures environ. Un script réussi **ne se relance pas** ; un script en échec est retenté jusqu'à 3 fois, aux vérifications suivantes.

Pour accélérer sur un PC pilote :
- **Paramètres → Comptes → Accès Professionnel ou Scolaire** → le compte de l'entreprise → **Informations** → **Synchroniser** ;
- ou, en PowerShell administrateur : `Restart-Service IntuneManagementExtension`.

6. Une fois les PC pilotes en **Réussite** (étape 7) et le site ouvert sans avertissement, revenez sur le script → **Propriétés** → **Affectations** → **Modifier** → ajoutez `Celluloplast - Certificat HTTPS - Tous`.

## Étape 6 (facultative) : correction automatique

Le script de plateforme ne s'exécute qu'une fois. Si quelqu'un retire le certificat plus tard, il ne revient pas. Les **corrections** (Remediations) vérifient le PC selon un planning et réinstallent le certificat s'il manque.

**Licence requise :** Windows Entreprise E3 ou E5 (comprise dans Microsoft 365 E3/E5), Education A3/A5, ou Windows VDA. Si l'onglet « Corrections » signale une licence manquante, contentez-vous de l'étape 5.

1. **Appareils** → **Scripts et corrections** → onglet **Corrections** → **+ Créer**.
2. Nom : `Celluloplast - Certificat racine HTTPS (contrôle)`.
3. **Paramètres** :

   | Paramètre | Valeur |
   |---|---|
   | Fichier de script de détection | `Detecter-CertificatCelluloplast.ps1` |
   | Fichier de script de correction | `Installer-CertificatCelluloplast.ps1` |
   | Exécuter ce script à l'aide des informations d'identification de connexion | **Non** |
   | Appliquer la vérification de la signature du script | **Non** |
   | Exécuter le script dans PowerShell 64 bits | **Oui** |

4. **Affectations** : le même groupe ; **Planification** : **Quotidien**, à une heure de bureau.
5. Le tableau de bord de la correction affiche ensuite, par PC : « Sans problème » (certificat présent), « Problème résolu » (certificat réinstallé) ou « Échec ». La colonne de sortie reprend le message du script (`Conforme : …` / `Non conforme : …`).

Les étapes 5 et 6 peuvent coexister : le script d'installation ne fait rien si le certificat est déjà là.

## Étape 7 : suivre le déploiement

**Dans Intune :** **Appareils → Scripts et corrections → Scripts de plateforme** → `Celluloplast - Certificat racine HTTPS` → **État de l'appareil**. Chaque PC passe par « En attente », puis « Réussite » ou « Échec ».

**Sur un PC :**

| Vérification | Commande ou emplacement | Résultat attendu |
|---|---|---|
| Journal du script | `C:\ProgramData\Celluloplast\certificat-racine.log` | `Ajouté : …` ou `Déjà présent : …`, puis `Terminé avec succès.` |
| Certificat installé | `Test-Path Cert:\LocalMachine\Root\<EMPREINTE-SHA1>` | `True` |
| Stratégie Firefox | `Get-ItemProperty HKLM:\SOFTWARE\Policies\Mozilla\Firefox\Certificates` | `ImportEnterpriseRoots : 1` |
| Navigateur | Fermer puis rouvrir Edge ou Chrome, ouvrir `https://<IP-du-serveur>/` | Cadenas, sans avertissement |
| Journaux Intune | `C:\ProgramData\Microsoft\IntuneManagementExtension\Logs\IntuneManagementExtension.log` et `AgentExecutor.log` | Cherchez « Celluloplast » ou l'identifiant du script |

## Dépannage

| Symptôme | Cause probable | Que faire |
|---|---|---|
| Le PC reste « En attente » | PC éteint, non synchronisé, ou pas dans le groupe | Vérifiez qu'il est dans le groupe et qu'il est inscrit. Synchronisez-le (étape 5), ou attendez le cycle de 8 h |
| Aucun dossier `IntuneManagementExtension` sur le PC | Extension pas encore installée, ou PC non inscrit (seulement enregistré) | Vérifiez l'inscription (Accès Professionnel ou Scolaire). L'extension arrive avec le premier script affecté |
| « Échec », journal : `empreinte inattendue` | Script modifié après génération (copier-coller, encodage) | Régénérez-le (étape 2) et téléversez le fichier tel quel |
| « Échec », journal : accès refusé | Script exécuté avec l'identité de l'utilisateur | Mettez « informations d'identification de l'utilisateur connecté » à **Non** |
| Pas de journal dans `C:\ProgramData\Celluloplast` | Le script n'a pas démarré | Lisez `AgentExecutor.log`. Vérifiez « vérification de la signature » = **Non** |
| Firefox avertit encore | Stratégie écrite en 32 bits, ou Firefox pas redémarré | « hôte PowerShell 64 bits » = **Oui**. Fermez complètement Firefox. Vérifiez `about:policies` |
| Edge ou Chrome avertit encore alors que le certificat est installé | Navigateur resté ouvert | Fermez **toutes** ses fenêtres (y compris en arrière-plan : `edge://settings/system`) ou redémarrez le PC |
| Avertissement « nom ne correspond pas » | Le site est ouvert par un autre nom ou une autre adresse | Utilisez exactement `https://<IP-du-serveur>/` |
| Tous les PC repassent en avertissement du jour au lendemain | L'autorité du serveur a changé (réinstallation sans restaurer les secrets) | Voir la section suivante |

## Autre méthode sans script : le profil « Certificat approuvé »

Intune sait aussi pousser un certificat racine sans aucun script. Cette méthode suffit pour Edge et Chrome ; pour Firefox, il faut ajouter la stratégie séparément.

1. Renommez `celluloplast-root.crt` en `celluloplast-root.cer`.
2. **Appareils** → **Configuration** → **+ Créer** → **Nouvelle stratégie** → Plateforme **Windows 10 et versions ultérieures** → Type de profil **Modèles** → **Certificat approuvé**.
3. Nom : `Celluloplast - Certificat racine HTTPS (profil)`. Fichier : `celluloplast-root.cer`. **Magasin de destination** : **Magasin de certificats de l'ordinateur - Racine**.
4. Affectez les mêmes groupes.

N'utilisez qu'une méthode à la fois pour un même groupe, afin de lire un seul état de déploiement. Le résultat sur le PC est le même.

## Changement d'autorité, retrait

La racine est valable 10 ans. Elle ne change que si le serveur est réinstallé **sans** restaurer l'archive des secrets (`secrets-<date>.tar.gz.gpg`, qui contient l'autorité de Caddy ; voir `deploy/README.md`). Dans ce cas :

1. Refaites les étapes 1 et 2 avec la nouvelle racine : la nouvelle empreinte apparaît dans `empreintes.txt`.
2. Dans Intune, ouvrez le script de plateforme → **Propriétés** → **Paramètres du script** → téléversez le nouveau `Installer-CertificatCelluloplast.ps1`. Un script modifié se relance sur tous les PC affectés. Faites de même pour la correction de l'étape 6.
3. Pour retirer l'ancienne racine : générez les scripts avec l'**ancien** `celluloplast-root.crt` dans un autre dossier, puis déployez `Desinstaller-CertificatCelluloplast.ps1` comme script de plateforme distinct (en SYSTEM, 64 bits).

Pour arrêter le déploiement sans rien retirer des PC, supprimez les affectations du script. Pour retirer le certificat, déployez `Desinstaller-CertificatCelluloplast.ps1`.

## Règles de sécurité

- Seul le **certificat public** (`root.crt`) quitte le serveur. La clé privée de l'autorité (`root.key`) n'est jamais copiée, ni dans un script, ni dans Intune, ni dans Git.
- Comparez toujours les empreintes (étape 2) avant de déployer : un certificat racine installé partout permettrait à son détenteur d'usurper n'importe quel site.
- Ne déposez pas les scripts générés dans le dépôt Git : ils sont propres à votre serveur. Seul le générateur y figure.
- N'affectez pas le script à des PC qui n'ont pas à ouvrir l'application, s'il en existe.
- Les PC non gérés par Intune s'équipent à la main : `certutil -addstore -f Root celluloplast-root.crt` en PowerShell administrateur, ou par GPO (voir `deploy/README.md`, § 4).
