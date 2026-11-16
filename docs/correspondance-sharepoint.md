# Correspondance SharePoint → application

Ce document indique la destination des **184 champs métier** des 14 listes de l'ancienne application. Les colonnes techniques de SharePoint en sont exclues. Mise à jour du 16/11/2026.

Chaque champ se lit selon deux questions distinctes :

1. **Nouvelles saisies** : où cette information vit-elle désormais dans l'application ?
2. **Valeur ancienne après reprise** : que devient la valeur déjà saisie dans SharePoint ?

## Que devient la valeur ancienne ?

| Sort | Champs | Ce que cela veut dire |
|---|---|---|
| Conservée et exploitable | 102 | Reprise dans un champ structuré, utilisable par l'application : filtres, règles, remises, restitutions. |
| Conservée en texte seulement | 22 | Visible sur la fiche, le mouvement ou la session, mais aucune règle ne l'utilise : pas de filtre, pas de preuve, pas de tâche. |
| Consultable uniquement dans SharePoint | 55 | L'ancienne application, gardée en lecture seule, reste la seule source. Cela concerne les dossiers, les tâches et les modèles. |
| Non reprise | 5 | Valeur technique ou recalculée. Elle n'est stockée nulle part et ne manque à aucun usage. |

Hors listes :
- **Documents et Pièces jointes** (anciennes décharges signées, autres pièces) : **consultables uniquement dans SharePoint**. Ils ne sont ni importés ni rattachés aux mouvements repris (hypothèse H3).
- Une garde reprise n'a pas de décharge dans l'application. Aucune décharge n'est fabriquée pour une remise ancienne.
- Une référence fiable est conservée et affichée aux RH, ADMIN et IT :
  - un lien `LienJustificatif` vers `https://<site>.sharepoint.com/…` ;
  - ou la colonne `Attachments` cochée, avec l'`ID` de l'élément Affectations.

  Sans cette référence, l'application affiche « Remise reprise de SharePoint — justificatif à rechercher dans l'ancien système ».

## Couverture des nouvelles saisies

| Situation | Champs |
|---|---|
| Déjà couvert | 119 |
| Couvert autrement, sans perte | 41 |
| Partiellement couvert | 3 |
| À livrer | 3 |
| Remplacé par une décision métier | 16 |
| Point à arbitrer | 2 |

## Les 16 décisions métier : confirmées ou en attente

Répartition :
- 7 décisions sont confirmées par le cahier des charges (source citée) ;
- 1 applique une règle provisoire que vous avez donnée le 16/11 ;
- 8 sont des **hypothèses en attente de votre accord**.

| Champ | Statut | Source ou raison |
|---|---|---|
| `Affectations.Dossier` | Hypothèse en attente de votre accord | 02-Technique, T08 : « Préserver identifiants de liens, références et dates » : l'abandon du lien contredirait le cahier. En attendant : dossier d'origine gardé en texte. |
| `Affectations.Title` | Hypothèse en attente de votre accord | 02-Technique, T08 : « Préserver identifiants de liens, références et dates ». En attendant : ancienne référence gardée en texte. |
| `AffectationsVehicules.Dossier` | Hypothèse en attente de votre accord | 02-Technique, T08 : « Préserver identifiants de liens, références et dates ». En attendant : texte. |
| `AffectationsVehicules.Title` | Hypothèse en attente de votre accord | 02-Technique, T08 : « Préserver identifiants de liens, références et dates ». En attendant : texte. |
| `Dossiers.BesoinsValides` | Confirmée par le cahier | 01-Metier, M01 : « Aucune seconde validation ni bouton Lancer ». Les anciennes validations restent consultables dans SharePoint. |
| `Dossiers.BesoinsValidesParEmail` | Confirmée par le cahier | 01-Metier, M01 : « Aucune seconde validation ni bouton Lancer » |
| `Dossiers.DateValidationBesoins` | Confirmée par le cahier | 01-Metier, M01 : « Aucune seconde validation ni bouton Lancer » |
| `Dossiers.MessageriePrevue` | Confirmée par le cahier | 01-Metier, M01 : « RH ne choisit pas cPanel ou Microsoft 365 » |
| `Dossiers.VersionProcessus` | Hypothèse en attente de votre accord | Sans enjeu métier : valeur technique non reprise. |
| `Formations.Dossier` | Hypothèse en attente de votre accord | 02-Technique, T08 : « Préserver identifiants de liens, références et dates ». En attendant : texte. |
| `Formations.Title` | Hypothèse en attente de votre accord | 02-Technique, T08 : « Préserver identifiants de liens, références et dates ». En attendant : texte. |
| `ModelesTaches.OrdreTache` | Confirmée par le cahier | 01-Metier, M02 : « Un autre peut préparer l'adresse pendant que le premier prépare le PC. Seul un prérequis concret bloque ». Un ordre d'affichage paramétrable reste à livrer. |
| `Personnel.BesoinEmailPro` | Hypothèse en attente de votre accord | Un besoin appartient à un dossier, pas à la fiche. La valeur est gardée en texte en attendant votre accord. |
| `ProfilsPoste.MessageriePrevue` | Confirmée par le cahier | 01-Metier, M01 : « RH ne choisit pas cPanel ou Microsoft 365 » |
| `Taches.OrdreTache` | Confirmée par le cahier | 01-Metier, M02 : « Un autre peut préparer l'adresse pendant que le premier prépare le PC. Seul un prérequis concret bloque » |
| `UtilisateursApp.Role` | Règle provisoire du 16/11 | Règle provisoire du 16/11 : aucun droit automatique pour Manager, Moyens généraux et Responsable. |

## Hypothèses générales

> **Sans objet depuis le 16/11/2026** : l'ancienne application n'a jamais été en production. Ses dossiers, tâches et documents étaient des essais ; il n'y a pas d'historique à préserver (voir `docs/pilote.md`, §0).


- **H1 — anciens dossiers et tâches.** Les dossiers, tâches et modèles SharePoint ne sont pas recréés ; ils restent consultables en lecture seule.
  - Le cahier (T08) demande au contraire de « préserver identifiants de liens, références et dates » et de « contrôler les dossiers en cours ».
  - En attendant votre décision, la référence et le dossier d'origine sont gardés **en texte** sur chaque affectation et chaque session reprise.
  - **Non validé** : il manque un dossier « historique » consultable dans l'application.
- **H2 — dossiers en cours.** Faut-il rouvrir à la main chaque arrivée, mutation ou départ encore ouvert au gel ?
  - **Non validé.** À trancher au vu de leur nombre au moment du gel.
  - Rien n'est rouvert automatiquement : une reprise ne crée ni dossier, ni tâche, ni avis.
- **H3 — documents.** Les anciennes décharges et pièces ne sont pas importées.
  - Une remise ancienne reprise n'a donc pas de décharge dans l'application.
  - Le scan signé reste dans la bibliothèque SharePoint.

## Affectations (14 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `AttribueParEmail` | AttribueParEmail | Couvert autrement, sans perte (lot suivant, à recetter) | Opérateur de la remise | IT, ADMIN | **Conservée en texte seulement** : Idem |
| `AttribueParNom` | AttribueParNom | Couvert autrement, sans perte (lot suivant, à recetter) | Opérateur de la remise | IT, ADMIN | **Conservée en texte seulement** : « attribué par … » dans l'observation |
| `DateAttribution` | DateAttribution | Déjà couvert (lot suivant, à recetter) | Date de remise | IT, ADMIN | **Conservée et exploitable** : Reprise |
| `DateRestitution` | DateRestitution | Déjà couvert (lot suivant, à recetter) | Date de restitution (historique) | IT, ADMIN | **Conservée et exploitable** : Affectation close reprise dans l'historique |
| `Dossier` | Dossier | Remplacé par une décision métier | Les anciens dossiers ne sont pas repris | — | **Conservée en texte seulement** : « dossier SharePoint … » dans l'observation ; le dossier lui-même n'est pas recréé |
| `Employe` | Employe | Déjà couvert (lot suivant, à recetter) | Salarié détenteur | IT, ADMIN | **Conservée et exploitable** : Par ancien ID ou titre repris ; sinon décision |
| `EtatRetour` | EtatRetour | Déjà couvert (lot suivant, à recetter) | État de la ligne restituée | IT, ADMIN | **Conservée et exploitable** : Repris |
| `Ligne` | Ligne | Déjà couvert (lot suivant, à recetter) | SIM de la ligne remise | IT, ADMIN | **Conservée et exploitable** : Rapprochée par numéro (SIM requise) |
| `Materiel` | Materiel | Déjà couvert (lot suivant, à recetter) | Actif remis (lien vérifié) | IT, ADMIN | **Conservée et exploitable** : Rapproché par n° d'inventaire |
| `Observations` | Observations | Déjà couvert (lot suivant, à recetter) | Observation du mouvement | IT, ADMIN | **Conservée et exploitable** : Reprise |
| `RecuParEmail` | RecuParEmail | Couvert autrement, sans perte (lot suivant, à recetter) | Opérateur de la restitution | IT, ADMIN | **Conservée en texte seulement** : Idem |
| `RecuParNom` | RecuParNom | Couvert autrement, sans perte (lot suivant, à recetter) | Opérateur de la restitution | IT, ADMIN | **Conservée en texte seulement** : « restitution reçue par … » dans l'observation |
| `Title` | Titre | Remplacé par une décision métier (lot suivant, à recetter) | Référence du mouvement MVT-00000 | — | **Conservée en texte seulement** : « réf. SharePoint … » dans l'observation du mouvement repris |
| `TypeObjet` | TypeObjet | Déjà couvert (lot suivant, à recetter) | Nature de la ligne (actif, SIM, accessoire) | IT, ADMIN | **Conservée et exploitable** : Repris |

## AffectationsVehicules (14 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `DateRemise` | DateRemise | Déjà couvert (lot suivant, à recetter) | Date de remise | RH, ADMIN | **Conservée et exploitable** : Reprise |
| `DateRestitution` | DateRestitution | Déjà couvert (lot suivant, à recetter) | Date de restitution | RH, ADMIN | **Conservée et exploitable** : Reprise |
| `Dossier` | Dossier | Remplacé par une décision métier | Anciens dossiers non repris | — | **Conservée en texte seulement** : « dossier SharePoint … » dans l'observation de retour |
| `Employe` | Employe | Déjà couvert (lot suivant, à recetter) | Salarié | RH, ADMIN | **Conservée et exploitable** : Par ancien ID ou titre repris |
| `EtatDepart` | EtatDepart | Déjà couvert (lot suivant, à recetter) | État à la remise | RH, ADMIN | **Conservée et exploitable** : Repris |
| `KmDepart` | KmDepart | Déjà couvert (lot suivant, à recetter) | Kilométrage à la remise | RH, ADMIN | **Conservée et exploitable** : Repris |
| `KmRetour` | KmRetour | Déjà couvert (lot suivant, à recetter) | Kilométrage au retour | RH, ADMIN | **Conservée et exploitable** : Repris |
| `ObservationsRetour` | ObservationsRetour | Déjà couvert (lot suivant, à recetter) | Observations au retour | RH, ADMIN | **Conservée et exploitable** : Reprises |
| `RecuParEmail` | RecuParEmail | Couvert autrement, sans perte (lot suivant, à recetter) | Opérateur du retour | RH, ADMIN | **Conservée en texte seulement** : Idem |
| `RecuParNom` | RecuParNom | Couvert autrement, sans perte (lot suivant, à recetter) | Opérateur du retour | RH, ADMIN | **Conservée en texte seulement** : « reçu par … » dans l'observation |
| `RemisParEmail` | RemisParEmail | Couvert autrement, sans perte (lot suivant, à recetter) | Opérateur de la remise | RH, ADMIN | **Conservée en texte seulement** : Idem |
| `RemisParNom` | RemisParNom | Couvert autrement, sans perte (lot suivant, à recetter) | Opérateur de la remise | RH, ADMIN | **Conservée en texte seulement** : « remis par … » dans l'observation |
| `Title` | Titre | Remplacé par une décision métier (lot suivant, à recetter) | Référence VEH-00000 | — | **Conservée en texte seulement** : « réf. SharePoint … » dans l'observation de retour |
| `Vehicule` | Vehicule | Déjà couvert (lot suivant, à recetter) | Véhicule du mouvement | RH, ADMIN | **Conservée et exploitable** : Rapproché par immatriculation |

## CatalogueFormations (7 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `Categorie` | Categorie | Déjà couvert | Catalogue : catégorie | FORMATION, ADMIN | **Conservée et exploitable** : Reprise |
| `Description` | Description | Déjà couvert | Catalogue : description | FORMATION, ADMIN | **Conservée et exploitable** : Reprise |
| `DureeHeures` | DureeHeures | Déjà couvert | Catalogue : durée | FORMATION, ADMIN | **Conservée et exploitable** : Reprise |
| `FormateurEmail` | FormateurEmail | Couvert autrement, sans perte | Intervenant par défaut (compte habilité FORMATION) | FORMATION, ADMIN | **Conservée et exploitable** : Relié si un compte FORMATION existe ; sinon gardé comme nom |
| `FormateurNom` | FormateurNom | Déjà couvert | Formations → catalogue : intervenant par défaut, compte ou simple nom | FORMATION, ADMIN | **Conservée en texte seulement** : Intervenant par défaut sans compte : nom conservé |
| `Support` | Support | Déjà couvert | Catalogue : lien vers le support | FORMATION, ADMIN | **Conservée et exploitable** : Repris (adresse du lien) |
| `Title` | Intitule | Déjà couvert | Formations → catalogue : intitulé | FORMATION, ADMIN | **Conservée et exploitable** : Repris |

## Dossiers (34 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `BesoinCarteVisite` | BesoinCarteVisite | Déjà couvert | Besoin de cartes (tâche Graphiste) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `BesoinEmailPro` | BesoinEmailPro | Déjà couvert | Besoin d'adresse professionnelle (Oui / Non / À préciser) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `BesoinIntellix` | BesoinIntellix | Déjà couvert | Besoin Intelix | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `BesoinLigneMobile` | BesoinLigneMobile | Déjà couvert | Besoin SIM / ligne (distinct du téléphone) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `BesoinPCPortable` | BesoinPCPortable | Couvert autrement, sans perte | Besoin PC + préférence fixe / portable / sans préférence | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `BesoinsValides` | BesoinsValides | Remplacé par une décision métier | Pas d'étape « Valider les besoins » : IT et Formation voient l'annonce immédiatement | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `BesoinsValidesParEmail` | BesoinsValidesParEmail | Remplacé par une décision métier | Idem (journal : auteur de chaque révision de besoin) | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `BesoinVehicule` | BesoinVehicule | Déjà couvert | Besoin véhicule (tâche RH) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `BesoinXena` | BesoinXena | Déjà couvert | Besoin Xena | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `CleCreation` | CleCreation | Couvert autrement, sans perte | Clé d'idempotence de la commande (aucune double création) | — | **Non reprise** : Clé technique de l'ancienne application |
| `CommentaireBesoins` | CommentaireBesoins | Couvert autrement, sans perte | Précision par besoin (suivable besoin par besoin) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Commentaires` | Commentaires | Déjà couvert | Note RH du dossier (RH et ADMIN seulement) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `DateEffet` | DateEffet | Déjà couvert | Date d'effet du dossier | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `DateValidationBesoins` | DateValidationBesoins | Remplacé par une décision métier | Idem (journal horodaté des révisions) | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `DemandeurEmail` | DemandeurEmail | Couvert autrement, sans perte | Auteur du dossier (compte nominatif) | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `DemandeurNom` | DemandeurNom | Couvert autrement, sans perte | Auteur du dossier (compte nominatif) | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `DepartementCible` | DepartementCible | Déjà couvert | Département à l'arrivée / département cible (mutation) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Employe` | Employe | Déjà couvert | Dossier rattaché à la fiche (jamais de seconde fiche) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `FonctionCible` | FonctionCible | Déjà couvert | Poste prévu (arrivée) / poste cible (mutation) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `FormationsRequises` | FormationsRequises | Déjà couvert | Sessions de formation du dossier | RH (demande), FORMATION (réalisation) | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `ManagerCible` | ManagerCible | Déjà couvert | Manager à l'arrivée / manager cible (mutation) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `MaterielRequis` | MaterielRequis | Déjà couvert | Besoins d'équipement distincts avec quantités (PC, écran, imprimante…) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `MessageriePrevue` | MessageriePrevue | Remplacé par une décision métier | Plateforme choisie par l'IT au moment de créer l'adresse | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `ProfilCible` | ProfilCible | Déjà couvert | Profil choisi à l'annonce / profil cible (mutation) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Progression` | Progression | Couvert autrement, sans perte | Calculée : actions ouvertes, quantités remises | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `SiteCible` | SiteCible | Déjà couvert | Site prévu (arrivée) / site cible (mutation) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `SituationApresJSON` | SituationApresJSON | Couvert autrement, sans perte (lot suivant, à recetter) | Mutation : cible et écarts « conserver, ajouter, reprendre, vérifier » | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `SituationAvantJSON` | SituationAvantJSON | Couvert autrement, sans perte (lot suivant, à recetter) | Mutation : photographie figée de la situation | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Statut` | Statut | Couvert autrement, sans perte | Ouvert, clos, annulé ; attente portée par la tâche | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `TachesEnRetard` | TachesEnRetard | Couvert autrement, sans perte | Calculé : états des lieux, tâches en retard | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Title` | Reference | Couvert autrement, sans perte | Référence lisible ONB-, DEM-, MUT-, OFF-00000 | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `TypeDossier` | TypeDossier | Déjà couvert | Type du dossier (arrivée, mutation, départ, demande) | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `VersionProcessus` | VersionProcessus | Remplacé par une décision métier | Version des migrations de l'application | — | **Non reprise** : Version technique de l'ancienne application |
| `WilayasCibles` | WilayasCibles | Partiellement couvert | Wilayas à l'arrivée : oui ; wilayas cibles d'une mutation : à livrer | RH, ADMIN | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |

## Formations (10 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `DatePrevue` | DatePrevue | Déjà couvert (lot suivant, à recetter) | Date prévue | FORMATION | **Conservée et exploitable** : Reprise |
| `DateRealisee` | DateRealisee | Déjà couvert (lot suivant, à recetter) | Date réelle | FORMATION | **Conservée et exploitable** : Reprise |
| `Dossier` | Dossier | Remplacé par une décision métier | Anciens dossiers non repris | — | **Conservée en texte seulement** : « dossier SharePoint … » dans l'observation de la session |
| `Employe` | Employe | Déjà couvert (lot suivant, à recetter) | Session : salarié | FORMATION | **Conservée et exploitable** : Par ancien ID ou titre repris |
| `Evaluation` | Evaluation | Déjà couvert | Évaluation : acquis, à revoir | FORMATION | **Conservée et exploitable** : Reprise |
| `FormateurEmail` | FormateurEmail | Couvert autrement, sans perte (lot suivant, à recetter) | Intervenant avec compte FORMATION | FORMATION | **Conservée et exploitable** : Relié si un compte FORMATION existe ; sinon gardé comme nom |
| `FormateurNom` | FormateurNom | Déjà couvert (lot suivant, à recetter) | Session : « animée par » (compte ou nom), distinct de « enregistrée par » | FORMATION | **Conservée en texte seulement** : « Animée par » : nom conservé sans compte |
| `Formation` | Formation | Déjà couvert (lot suivant, à recetter) | Session : formation du catalogue | FORMATION | **Conservée et exploitable** : Rapprochée par intitulé |
| `Statut` | Statut | Déjà couvert (lot suivant, à recetter) | À planifier, planifiée, réalisée, reportée, dispensée | FORMATION | **Conservée et exploitable** : Repris ; jamais « réalisée » sans source |
| `Title` | Titre | Remplacé par une décision métier | Session identifiée par formation + salarié | — | **Conservée en texte seulement** : « réf. SharePoint … » dans l'observation de la session |

## LignesMobiles (7 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `AttribueA` | AttribueA | Couvert autrement, sans perte (lot suivant, à recetter) | Titulaire déduit de la garde de la SIM | IT, ADMIN | **Conservée et exploitable** : Garde reprise si l'ICCID existe |
| `Forfait` | Forfait | Déjà couvert (lot suivant, à recetter) | Ligne : forfait (filtrable) | IT, ADMIN | **Conservée et exploitable** : Repris |
| `ICCID` | ICCID | Couvert autrement, sans perte (lot suivant, à recetter) | Carte SIM liée à la ligne | IT, ADMIN | **Conservée et exploitable** : Reprise ; jamais inventée si absente |
| `Operateur` | Operateur | Déjà couvert (lot suivant, à recetter) | Ligne : opérateur | IT, ADMIN | **Conservée et exploitable** : Repris |
| `Statut` | Statut | Couvert autrement, sans perte (lot suivant, à recetter) | État de la ligne ; « Attribuée » déduit de la SIM détenue | IT, ADMIN | **Conservée et exploitable** : Reprise |
| `Title` | Numero | Déjà couvert (lot suivant, à recetter) | Parc IT → SIM et lignes : numéro | IT, ADMIN | **Conservée et exploitable** : Repris (clé) |
| `TypeLigne` | TypeLigne | Déjà couvert (lot suivant, à recetter) | Ligne : Voix, Data, Voix + Data | IT, ADMIN | **Conservée et exploitable** : Repris |

## MaterielIT (12 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `AttribueA` | AttribueA | Couvert autrement, sans perte (lot suivant, à recetter) | Détenteur déduit de l'affectation active | IT, ADMIN | **Conservée et exploitable** : Garde reprise par un mouvement « reprise » (sans décharge) |
| `Categorie` | Categorie | Déjà couvert (lot suivant, à recetter) | Parc IT → type (15 catégories) | IT, ADMIN | **Conservée et exploitable** : Reprise ; catégorie inconnue rejetée |
| `DateAchat` | DateAchat | Déjà couvert (lot suivant, à recetter) | Fiche actif | IT, ADMIN | **Conservée et exploitable** : Reprise |
| `Etat` | Etat | Déjà couvert (lot suivant, à recetter) | État physique (neuf, bon, usé, HS), distinct du statut | IT, ADMIN | **Conservée et exploitable** : Repris |
| `FinGarantie` | FinGarantie | Déjà couvert (lot suivant, à recetter) | Fiche actif | IT, ADMIN | **Conservée et exploitable** : Reprise |
| `Marque` | Marque | Déjà couvert (lot suivant, à recetter) | Fiche actif | IT, ADMIN | **Conservée et exploitable** : Reprise |
| `Modele` | Modele | Déjà couvert (lot suivant, à recetter) | Fiche actif | IT, ADMIN | **Conservée et exploitable** : Repris |
| `NomIntune` | NomIntune | Déjà couvert (lot suivant, à recetter) | Fiche actif | IT, ADMIN | **Conservée et exploitable** : Repris |
| `NumSerie` | NumSerie | Déjà couvert (lot suivant, à recetter) | Fiche actif | IT, ADMIN | **Conservée et exploitable** : Repris |
| `Observations` | Observations | Déjà couvert (lot suivant, à recetter) | Notes de l'actif | IT, ADMIN | **Conservée et exploitable** : Reprises |
| `Statut` | Statut | Couvert autrement, sans perte (lot suivant, à recetter) | Statut d'usage ; « Attribué » déduit de la garde réelle | IT, ADMIN | **Conservée et exploitable** : « Attribué » reconstitué par AttribueA ou Affectations |
| `Title` | NumInventaire | Déjà couvert (lot suivant, à recetter) | Parc IT → n° d'inventaire | IT, ADMIN | **Conservée et exploitable** : Repris (clé) |

## ModelesTaches (8 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `ActionTache` | ActionTache | Couvert autrement, sans perte | Action déduite du besoin (remise, compte, carte, véhicule, restitution, coupure) | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Condition` | Condition | Couvert autrement, sans perte | Seul « Oui » crée une tâche (À préciser n'invente rien) | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `DelaiJours` | DelaiJours | À livrer | Échéance = date d'effet ; délai paramétrable à livrer | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Equipe` | Equipe | Déjà couvert | Équipe responsable : routage besoin → équipe | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Instructions` | Instructions | À livrer | Instructions par modèle de tâche : à livrer | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `OrdreTache` | OrdreTache | Remplacé par une décision métier | Pas d'ordre imposé : Formation n'attend pas l'IT ; ordre d'affichage à livrer | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Title` | Titre | Partiellement couvert | Intitulé de tâche généré par besoin (non paramétrable) | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `TypeDossier` | TypeDossier | Partiellement couvert | Tâches générées selon le type de dossier | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |

## Personnel (25 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `BesoinEmailPro` | BesoinEmailPro | Remplacé par une décision métier | Le besoin appartient au dossier (ONB, DEM), pas à la fiche | — | **Conservée en texte seulement** : Ajouté à la remarque de reprise ; ne crée aucun besoin |
| `CarteVisite` | CarteVisite | Point à arbitrer | Une carte se demande par une demande ponctuelle (Graphiste) | — | **Conservée en texte seulement** : Ajoutée à la remarque de reprise ; ni tâche ni preuve |
| `DateActivationM365` | DateActivationM365 | Déjà couvert | Date de vérification M365 | IT, ADMIN | **Conservée et exploitable** : Repris |
| `DateEntree` | DateEntree | Déjà couvert | Arrivée prévue (à venir) ou présence réelle confirmée (actif) | RH, ADMIN | **Conservée et exploitable** : Repris ; une date réelle confirmée n'est jamais réécrite par import |
| `DateSortie` | DateSortie | Déjà couvert | Date de sortie (sorti) ou sortie prévue (en départ) | RH, ADMIN | **Conservée et exploitable** : Repris |
| `Departement` | Departement | Déjà couvert | Fiche → situation ; filtre de la liste | RH, ADMIN | **Conservée et exploitable** : Repris (texte de la liste fermée historique) |
| `EmailPro` | EmailPro | Couvert autrement, sans perte | Fiche → comptes et accès (adresse réellement créée) | IT, ADMIN | **Conservée et exploitable** : Compte consigné, affiché « Adresse reprise — vérification non renseignée » : aucune date de vérification inventée |
| `Fonction` | Fonction | Déjà couvert | Fiche → poste réel (distinct du profil) | RH, ADMIN | **Conservée et exploitable** : Repris ; vide → « À préciser » |
| `LignePrincipale` | LignePrincipale | Déjà couvert | Fiche → lignes mobiles, marque « principale » | RH, IT, ADMIN (parmi les lignes réellement détenues) | **Conservée et exploitable** : Appliquée seulement si la SIM de cette ligne est détenue (reprendre LignesMobiles d'abord) |
| `M365Operationnel` | M365Operationnel | Déjà couvert | Fiche → « Compte Microsoft 365 opérationnel — vérifié » | IT, ADMIN | **Conservée et exploitable** : Repris avec sa date d'activation ; « Oui » sans date : non activé, gardé en texte |
| `Manager` | Manager | Déjà couvert | Fiche → manager choisi parmi les salariés (lien cliquable) | RH, ADMIN | **Conservée et exploitable** : Par ancien ID, sinon titre repris unique, sinon e-mail pro unique ; sinon « à rattacher » |
| `ManagerEmail` | ManagerEmail | Couvert autrement, sans perte | Sert uniquement à retrouver la fiche du manager | — | **Non reprise** : Sert seulement à retrouver la fiche du manager ; non stocké |
| `ManagerNom` | ManagerNom | Couvert autrement, sans perte | Affiché « à rattacher » tant qu'aucune fiche n'est reliée | RH, ADMIN | **Conservée en texte seulement** : Affiché « à rattacher » tant qu'aucune fiche n'est reliée |
| `Matricule` | Matricule | Déjà couvert | Fiche → identité (texte, « 00042 » conservé) | RH, ADMIN | **Conservée et exploitable** : Repris ; sert à l'identification, jamais le nom |
| `NomFamille` | NomFamille | Déjà couvert | Fiche → identité ; annonce d'arrivée | RH, ADMIN | **Conservée et exploitable** : Repris tel quel |
| `PlateformeMessagerie` | PlateformeMessagerie | Couvert autrement, sans perte | Plateforme du compte e-mail consigné (choisie par l'IT) | IT, ADMIN | **Conservée et exploitable** : « Inconnu » ou « Aucune » : aucun compte inventé |
| `PosteFixe` | PosteFixe | Point à arbitrer | Fiche → « Poste fixe » (ancien champ, sens à confirmer) | RH, ADMIN | **Conservée en texte seulement** : Valeur gardée telle quelle, sens à confirmer |
| `Prenom` | Prenom | Déjà couvert | Fiche → identité ; annonce d'arrivée | RH, ADMIN | **Conservée et exploitable** : Repris tel quel (accents et arabe conservés) |
| `ProfilPoste` | ProfilPoste | Déjà couvert | Fiche → profil de poste (référentiel) ; filtre de la liste | RH, ADMIN | **Conservée et exploitable** : Rapproché par intitulé après reprise de ProfilsPoste |
| `RemarqueImport` | RemarqueImport | Déjà couvert | Fiche → remarque de reprise (RH et ADMIN seulement) | RH, ADMIN | **Conservée en texte seulement** : Remarque de reprise (RH, ADMIN) |
| `Site` | Site | Déjà couvert | Fiche → situation : site de rattachement administratif | RH, ADMIN (mutation pour un changement) | **Conservée et exploitable** : Libellé rapproché du référentiel des sites ; site inconnu = ligne rejetée |
| `Statut` | Statut | Déjà couvert | Statut de la fiche : à venir, actif, en départ, sorti | RH, ADMIN (présence confirmée, en départ) ; sortie par le dossier OFF | **Conservée et exploitable** : « En départ » repris comme présent + indicateur « en départ » |
| `Title` | NomComplet | Couvert autrement, sans perte | Calculé (prénom + nom) sur la fiche et dans la liste | — | **Non reprise** : Recalculé à partir du prénom et du nom |
| `TypeContrat` | TypeContrat | Déjà couvert | Fiche → informations RH (RH et ADMIN seulement) | RH, ADMIN | **Conservée et exploitable** : CDI, CDD, Stage, Intérim, Apprentissage, Consultant |
| `Wilayas` | Wilayas | Déjà couvert | Fiche → wilayas couvertes (aucune, une ou plusieurs) ; filtre de la liste | RH, ADMIN | **Conservée et exploitable** : Rapprochées par libellé ou code après reprise de Wilayas |

## ProfilsPoste (15 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `AccesACreer` | AccesACreer | Déjà couvert | Profil → autres accès à préciser | RH, ADMIN | **Conservée en texte seulement** : Texte « autres accès à préciser » |
| `BesoinEmailPro` | BesoinEmailPro | Déjà couvert | Profil → besoin d'adresse professionnelle | RH, ADMIN | **Conservée et exploitable** : Repris |
| `BesoinIntellix` | BesoinIntellix | Déjà couvert | Profil → besoin Intelix | RH, ADMIN | **Conservée et exploitable** : Repris |
| `BesoinXena` | BesoinXena | Déjà couvert | Profil → besoin Xena | RH, ADMIN | **Conservée et exploitable** : Repris |
| `CarteVisite` | CarteVisite | Déjà couvert | Profil → besoin de cartes de visite | RH, ADMIN | **Conservée et exploitable** : Repris |
| `Departement` | Departement | Déjà couvert | Profil → département | RH, ADMIN | **Conservée et exploitable** : Repris |
| `Description` | Description | Déjà couvert | Profil → description | RH, ADMIN | **Conservée et exploitable** : Repris |
| `FonctionsTypes` | FonctionsTypes | Déjà couvert | Profil → fonctions types | RH, ADMIN | **Conservée et exploitable** : Repris |
| `FormationsRequises` | FormationsRequises | Déjà couvert | Profil → formations recommandées (entrées du catalogue) | RH, ADMIN | **Conservée et exploitable** : Rapprochées par intitulé après reprise du catalogue |
| `LigneMobile` | LigneMobile | Déjà couvert | Profil → besoin SIM / ligne | RH, ADMIN | **Conservée et exploitable** : Repris |
| `MaterielStandard` | MaterielStandard | Déjà couvert | Profil → besoins suggérés : PC (préférence), écran, tablette, imprimante, accessoire, téléphone, avec quantités | RH, ADMIN | **Conservée et exploitable** : Chaque matériel devient un besoin distinct ; valeur inconnue signalée |
| `MessageriePrevue` | MessageriePrevue | Remplacé par une décision métier | L'IT choisit la plateforme ; le profil n'en impose pas | — | **Non reprise** : Signalée à l'aperçu, non stockée |
| `Terrain` | Terrain | Déjà couvert | Profil → poste terrain | RH, ADMIN | **Conservée et exploitable** : Repris |
| `Title` | Intitule | Déjà couvert | Administration → Profils de poste : intitulé | RH, ADMIN | **Conservée et exploitable** : Repris |
| `Vehicule` | Vehicule | Déjà couvert | Profil → besoin véhicule (orienté vers le RH) | RH, ADMIN | **Conservée et exploitable** : Repris |

## Taches (15 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `ActionTache` | ActionTache | Couvert autrement, sans perte | Besoin ou contrôle de sortie lié | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `AssigneEmail` | AssigneEmail | Couvert autrement, sans perte | Responsable (compte nominatif) | Équipe | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `AssigneNom` | AssigneNom | Couvert autrement, sans perte | Responsable : prise en charge atomique | Équipe | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Commentaire` | Commentaire | Couvert autrement, sans perte | Observation de fin, raison d'attente | Équipe | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `DateRealisation` | DateRealisation | Déjà couvert | Date de fin, avec la preuve (remis, déjà détenu, exception, réalisé) | Équipe | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Dossier` | Dossier | Déjà couvert | Tâche rattachée à son dossier | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Echeance` | Echeance | Déjà couvert | Échéance de la tâche | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Employe` | Employe | Déjà couvert | Via le dossier | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Equipe` | Equipe | Déjà couvert | Équipe de la tâche (file « Mon travail ») | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Instructions` | Instructions | À livrer | Instructions venant du modèle : à livrer | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `OrdreTache` | OrdreTache | Remplacé par une décision métier | Aucun ordre imposé entre équipes | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `RealiseeParEmail` | RealiseeParEmail | Déjà couvert | Auteur de la fin de tâche | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `RealiseeParNom` | RealiseeParNom | Déjà couvert | Auteur de la fin de tâche | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Statut` | Statut | Couvert autrement, sans perte | À prendre, en cours, en attente, terminée, annulée ; « N/A » = exception motivée | Équipe responsable | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |
| `Title` | Titre | Déjà couvert | Tâche du dossier | — | **Consultable uniquement dans SharePoint** : Dossiers, tâches et modèles non recréés (hypothèse H1) |

## UtilisateursApp (5 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `Actif` | Actif | Déjà couvert | Compte actif ou désactivé | ADMIN | **Conservée et exploitable** : Repris ; aucun mot de passe : lien d'activation à générer |
| `Email` | Email | Déjà couvert | Identifiant de connexion et adresse des avis | ADMIN | **Conservée et exploitable** : Repris (clé) |
| `Equipe` | Equipe | Déjà couvert | Rôles RH, IT, Graphiste, FORMATION ; le manager est désigné sur la fiche sans compte | ADMIN | **Conservée et exploitable** : RH, IT, Graphiste, Formateur ; « Manager » et « Moyens généraux » : aucun droit, compte non créé s'il n'a pas d'autre rôle |
| `Role` | Role | Remplacé par une décision métier | « Admin » → ADMIN ; « Responsable » et « Utilisateur » ne donnent aucun droit | ADMIN | **Conservée et exploitable** : « Admin » seul est repris ; « Responsable » et « Utilisateur » : aucun droit |
| `Title` | NomPrenom | Déjà couvert | Administration → Utilisateurs : nom | ADMIN | **Conservée et exploitable** : Repris |

## Vehicules (11 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `AttribueA` | AttribueA | Couvert autrement, sans perte (lot suivant, à recetter) | Titulaire déduit de l'affectation active | RH, ADMIN | **Conservée et exploitable** : Garde reprise |
| `DateFinLocation` | DateFinLocation | Déjà couvert (lot suivant, à recetter) | Fin du contrat ; alerte avant échéance | RH, ADMIN | **Conservée et exploitable** : Reprise |
| `EcheanceAssurance` | EcheanceAssurance | Déjà couvert (lot suivant, à recetter) | Pièce « assurance » avec échéance | RH, ADMIN | **Conservée et exploitable** : Reprise comme pièce |
| `EcheanceCT` | EcheanceCT | Déjà couvert (lot suivant, à recetter) | Pièce « contrôle technique » avec échéance | RH, ADMIN | **Conservée et exploitable** : Reprise comme pièce |
| `Kilometrage` | Kilometrage | Déjà couvert (lot suivant, à recetter) | Compteur (contrôlé à chaque mouvement) | RH, ADMIN | **Conservée et exploitable** : Repris |
| `Loueur` | Loueur | Déjà couvert (lot suivant, à recetter) | Contrat de location | RH, ADMIN | **Conservée et exploitable** : Repris |
| `Marque` | Marque | Déjà couvert (lot suivant, à recetter) | Fiche véhicule | RH, ADMIN | **Conservée et exploitable** : Reprise |
| `Modele` | Modele | Déjà couvert (lot suivant, à recetter) | Fiche véhicule | RH, ADMIN | **Conservée et exploitable** : Repris |
| `Statut` | Statut | Couvert autrement, sans perte (lot suivant, à recetter) | Disponible, affecté, immobilisé, rendu au loueur, réformé | RH, ADMIN | **Conservée et exploitable** : « Attribué » déduit de la garde |
| `Title` | Immatriculation | Déjà couvert (lot suivant, à recetter) | Véhicules : immatriculation | RH, ADMIN | **Conservée et exploitable** : Reprise (clé) |
| `TypeVehicule` | TypeVehicule | Déjà couvert (lot suivant, à recetter) | Propriété ou location | RH, ADMIN | **Conservée et exploitable** : Repris |

## Wilayas (7 champs)

| Champ | Libellé | Nouvelles saisies | Où le saisir / le consulter | Qui modifie | Valeur ancienne après reprise |
|---|---|---|---|---|---|
| `CodeWilaya` | CodeWilaya | Déjà couvert | Administration → Profils et wilayas : code | ADMIN | **Conservée et exploitable** : Clé de reprise |
| `Latitude` | Latitude | Déjà couvert | Coordonnées (si présentes à la source) | ADMIN | **Conservée et exploitable** : Reprises ; jamais complétées automatiquement |
| `Longitude` | Longitude | Déjà couvert | Coordonnées (si présentes à la source) | ADMIN | **Conservée et exploitable** : Reprises |
| `OrdreRegion` | OrdreRegion | Déjà couvert | Ordre d'affichage | ADMIN | **Conservée et exploitable** : Repris |
| `Region` | Region | Déjà couvert | Région de la wilaya (jamais déduite du site d'un salarié) | ADMIN | **Conservée et exploitable** : Reprise |
| `Title` | Libelle | Couvert autrement, sans perte | Libellé affiché (code + nom) | ADMIN | **Conservée et exploitable** : Utilisé si « Wilaya » est vide |
| `Wilaya` | Wilaya | Déjà couvert | Libellé de la wilaya | ADMIN | **Conservée et exploitable** : Repris |

## Règles provisoires du 16/11 appliquées

- **PosteFixe.**
  - Dans une nouvelle demande, « PC fixe » signifie ordinateur de bureau (préférence du besoin PC).
  - L'ancienne colonne de la fiche est conservée telle quelle, affichée « sens à confirmer ».
- **CarteVisite.**
  - Une ancienne case cochée est gardée en texte dans la remarque de reprise : « n'atteste ni impression ni remise ».
  - Elle ne crée ni tâche, ni preuve, ni carte détenue.
- **Manager, Moyens généraux, Responsable.**
  - Aucun droit automatique : un compte qui n'a que ces profils n'est pas créé.
  - Un manager se désigne sur la fiche du salarié sans obtenir d'accès.
- **Formation.**
  - « Animée par » (compte FORMATION ou simple nom, sans compte créé) est distinct de « enregistrée par » (l'utilisateur qui saisit).
  - La reprise conserve FormateurNom même sans compte.
