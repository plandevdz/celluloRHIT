# Pilote sur un site : règle, périmètre, preuves anciennes, limites

État au 16/11/2026 (branche `lot3-complet`, PR n° 2 en brouillon).

Tout a été vérifié sur une **instance isolée**, avec des données fictives (`@exemple.test`), `MAIL_MODE=journal`, l'archivage désactivé et `MODULES_DESACTIVES=pilote`. Il n'y a eu ni envoi réel, ni accès à la production, ni déploiement, ni reprise de données réelles.

La fiche à suivre soi-même est dans [recette-pilote.md](recette-pilote.md).

## 1. Règle du pilote : seules les nouvelles opérations

Décision de Celluloplast, **valable pour le pilote seulement**. Elle ne valide pas l'abandon des liens historiques, que le cahier T08 contredit (voir [correspondance-sharepoint.md](correspondance-sharepoint.md), hypothèse H1).

- À compter de la **date de démarrage** choisie par Celluloplast, toute **nouvelle** arrivée et toute **nouvelle** demande ponctuelle du site pilote sont ouvertes dans la nouvelle application.
- Un dossier **déjà ouvert dans Power Apps** y est terminé par son équipe habituelle :
  - pas de ressaisie systématique ;
  - pas de réouverture ;
  - pas de deuxième notification.
- Les salariés déjà présents peuvent avoir leur fiche et leurs biens repris dans la nouvelle application. Leurs anciens dossiers ne sont pas recréés : la reprise ne crée ni dossier, ni tâche, ni avis.

**Exemple 1 : arrivée commencée la veille du pilote.**
- Une arrivée a été annoncée dans Power Apps la veille de la date de démarrage, pour une recrue attendue la semaine suivante.
- Elle se termine dans Power Apps : remises, compte email et formations sont saisis là-bas par les équipes habituelles, qui ne reçoivent aucun avis de la nouvelle application.
- La nouvelle application n'ouvre pas d'ONB pour cette recrue.
- Si sa fiche est reprise plus tard (export Personnel), elle arrive sans dossier, sans tâche et sans avis. Ses biens, s'ils sont repris, apparaissent comme « Remise reprise de SharePoint ».

**Exemple 2 : demande de carte de visite reçue le lendemain du démarrage.**
- La demande concerne un salarié déjà présent. C'est une nouvelle opération : le RH l'ouvre dans la nouvelle application, depuis la fiche → « Nouvelle demande » → Cartes de visite.
- Seul le Graphiste est avisé (en mode journal : avis consigné, aucun envoi).
- Si la fiche du salarié n'est pas encore dans l'application, le RH la crée d'abord (fiche hors parcours, aucun avis), ou elle est reprise.
- Une demande de carte reçue **avant** le démarrage et encore ouverte reste traitée dans Power Apps.

## 2. Ce que voient les utilisateurs en mode pilote

Réglage : `MODULES_DESACTIVES=pilote`. C'est la valeur par défaut du `docker-compose.yml`.

| Fonction | En mode pilote | Vérifié |
|---|---|---|
| Personnel, profils de poste, arrivées, Mon travail, formations, demandes ponctuelles | Ouvert | Navigateur |
| Parc IT : actifs, accessoires, SIM ; remises, restitutions, décharges, scans | Ouvert. Le **scan d'un bien** à la remise reste disponible | Navigateur (le code PC-201 est reconnu) |
| Excel du personnel | Ouvert | Navigateur (15/11) |
| Reprise SharePoint | ADMIN seulement | Navigateur |
| Véhicules | Menu masqué ; `/vehicules` affiche « Fonction non ouverte » ; API refusée ; aucune alerte d'échéance ; besoin véhicule retiré des formulaires et refusé par l'API | Navigateur + test `pilote` |
| Logiciels et licences | Onglet masqué ; section « Licences » de la fiche masquée ; API refusée ; aucune alerte de renouvellement | Navigateur + test |
| Étiquettes et campagnes QR | Onglet « Inventaires QR » et boutons « Imprimer l'étiquette » masqués ; API refusée | Navigateur + test |
| Mutation | Bouton masqué, ainsi que « Correction de poste datée » ; API refusée ; aucune mutation appliquée par le worker | Navigateur + test |
| Départ | Bouton et sections de l'accueil masqués ; API refusée ; aucune coupure ni relance automatique | Navigateur + test |
| États | Menu masqué ; `/etats` affiche « Fonction non ouverte » ; export refusé. L'accueil RH garde « Signatures en attente » | Navigateur + test |
| Exchange | `MAIL_MODE=journal` : aucun envoi | Navigateur (5 avis, tous « journalisés ») |
| Anciens avis de test | Le jour où Exchange sera activé, aucun avis créé pendant la période en mode journal ne part : il est consigné | Test `pilote` (pas de vrai Exchange) |
| Archivage SharePoint | Désactivé : copie locale seulement | Navigateur |

Une demande d'arrivée avec un profil qui prévoit un véhicule :
- le RH voit l'avertissement « Ce profil prévoit un véhicule : non géré dans l'application pendant le pilote » ;
- aucune tâche RH n'est créée ;
- la clôture n'est pas bloquée.

## 3. Preuves anciennes

- Les décharges déjà signées **restent dans SharePoint**. Elles ne sont pas importées et aucun PDF n'est généré pour une remise ancienne.
- Si l'export Affectations fournit une **référence fiable**, l'utilisateur RH, ADMIN ou IT la retrouve :
  - **lien vérifié** : colonne `LienJustificatif` contenant une adresse `https://<site>.sharepoint.com/…`. La fiche affiche « Justificatif dans SharePoint : ouvrir l'ancien document » ;
  - **pièce jointe** : colonne `Attachments` cochée et colonne `ID` présente. La fiche affiche « Justificatif dans SharePoint : pièce jointe de l'élément n° 701 de la liste Affectations ».
- Sinon, la fiche affiche « Remise reprise de SharePoint — justificatif à rechercher dans l'ancien système ». Un lien qui ne pointe pas vers SharePoint est ignoré.
- **« Signé » n'est jamais affiché** pour une garde reprise : seule une décharge produite par l'application, avec son scan, porte cet état.
- Le Graphiste ne voit ni les biens ni ces références (vérifié : 403 sur les biens, aucune référence dans la fiche).
- La **restitution** d'un bien repris produit sa propre décharge de restitution, qui cite la remise d'origine (ex. « MVT-00001 »).

**Adresse email reprise** : « Adresse reprise — vérification non renseignée ».
- Aucune date n'est inventée, et « Microsoft 365 opérationnel » n'est pas activé.
- Un ancien « M365 opérationnel : Oui » sans date est gardé en texte dans la remarque de reprise.
- L'IT peut ensuite choisir « Actif (vérifié) » : c'est seulement à ce moment qu'une date de vérification est posée.
- Passer un compte « à vérifier » ne pose plus de date de vérification.

**Dépôt d'un scan** : l'état de la décharge passe à « Signé (scan déposé) » dès la réponse du serveur. L'écran confirme « Scan enregistré (version N, fichier) : décharge signée », sur les écrans de remise, de restitution et du dossier.

## 4. Écarts et limites

### Corrigés dans cette passe

| Écart | Avant | Maintenant |
|---|---|---|
| Modules non recettés visibles | Un RH pouvait lancer un départ ou une mutation | Masqués, liens directs et API refusés, traitements arrêtés |
| Profil avec véhicule | Tâche RH impossible à traiter, clôture bloquée | Aucune tâche ; avertissement ; besoin refusé par l'API |
| Adresse reprise | « vérifié le » + date de reprise | « Adresse reprise — vérification non renseignée » |
| M365 « Oui » sans date | Activé à la date de reprise | Non activé, gardé en texte |
| « À vérifier » sur un compte | Posait une date de vérification | Ne date rien |
| Scan déposé | État inchangé jusqu'au rechargement | État et confirmation immédiats |
| Preuve d'une garde reprise | « sans décharge dans l'application » | Lien ou référence SharePoint, sinon « justificatif à rechercher dans l'ancien système » |
| Anciens avis de test | Pouvaient partir à l'activation d'Exchange | Consignés, jamais envoyés |

### Restent, sans bloquer le pilote

| Limite | Exemple concret |
|---|---|
| Wilayas cibles d'une mutation non livrées (module fermé) | — pendant le pilote |
| Modèles de tâches non paramétrables | La Formation ne peut pas joindre de consignes standard à une session |
| Un compte « Manager » refusé compte encore parmi les rejets de la reprise | L'ADMIN voit un « rejet » voulu |
| Le profil de poste peut toujours cocher « Véhicule » | Sans effet pendant le pilote : la suggestion est écartée à l'annonce, avec un avertissement |
| Le lien SharePoint n'est pas vérifié en ligne (le document peut avoir été déplacé) | L'IT clique sur « ouvrir l'ancien document » et tombe sur une page introuvable : il cherche alors dans SharePoint |
| Non validé sur site : impression A4, douchette, classeur ouvert dans Excel et LibreOffice, noms arabes dans les PDF | — |

## 5. Décision restant à votre main

- **Date de démarrage du pilote** et site retenu.
- Sur l'historique (H1, H2, H3 de la correspondance), aucune autre décision n'est nécessaire pour le pilote : la règle du §1 suffit. La question reste ouverte pour la bascule générale.
