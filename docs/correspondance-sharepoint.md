# Correspondance SharePoint → application

Destination des **184 champs métier** des 14 listes de l'ancienne application (export du 15/11/2026, colonnes techniques de SharePoint exclues). Chaque champ est classé dans une seule situation.

| Situation | Champs |
|---|---|
| Déjà couvert | 116 |
| Couvert autrement, sans perte | 41 |
| Partiellement couvert | 6 |
| À livrer | 3 |
| Remplacé par une décision métier | 16 |
| Point à arbitrer | 2 |

« Disponible » : **1re livraison** = utilisable avec la fiche personnel, les profils, l'arrivée et l'Excel ; **livré, recette du lot suivant** = présent dans la même branche, à recetter avec son lot (parc, lignes, véhicules, mutation…) ; « — » = sans objet (décision) ou non livré.

Les données elles-mêmes (profils réels, modèles de tâches, wilayas) ne figurent pas dans l'export : rien n'a été inventé. Le comportement des anciens écrans Power Apps (.msapp) n'est pas décrit par l'export : les points marqués « à arbitrer » sont à confirmer avec l'équipe.

**Documents et Pièces jointes** : ces bibliothèques ne sont pas reprises automatiquement. Leur contenu (décharges signées, mais aussi d'autres pièces) doit être inventorié avant toute reprise ; les nouvelles décharges sont produites et archivées par l'application.

**Dossiers, Taches, ModelesTaches** : les dossiers et tâches historiques ne sont pas recréés (aucune régénération de tâche ni d'avis). Les parcours en cours sont rouverts dans l'application ; l'ancienne application reste consultable en lecture seule.

## Affectations (14 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `AttribueParEmail` | AttribueParEmail | Couvert autrement, sans perte | Livré, recette du lot suivant | Opérateur de la remise | IT, ADMIN | Conservé dans l'observation |
| `AttribueParNom` | AttribueParNom | Couvert autrement, sans perte | Livré, recette du lot suivant | Opérateur de la remise | IT, ADMIN | Conservé dans l'observation de la reprise |
| `DateAttribution` | DateAttribution | Déjà couvert | Livré, recette du lot suivant | Date de remise | IT, ADMIN | Reprise |
| `DateRestitution` | DateRestitution | Déjà couvert | Livré, recette du lot suivant | Date de restitution (historique) | IT, ADMIN | Affectation close reprise dans l'historique |
| `Dossier` | Dossier | Remplacé par une décision métier | — | Les anciens dossiers ne sont pas repris | — | Lien non repris |
| `Employe` | Employe | Déjà couvert | Livré, recette du lot suivant | Salarié détenteur | IT, ADMIN | Par ancien ID ou titre repris ; sinon décision |
| `EtatRetour` | EtatRetour | Déjà couvert | Livré, recette du lot suivant | État de la ligne restituée | IT, ADMIN | Repris |
| `Ligne` | Ligne | Déjà couvert | Livré, recette du lot suivant | SIM de la ligne remise | IT, ADMIN | Rapprochée par numéro (SIM requise) |
| `Materiel` | Materiel | Déjà couvert | Livré, recette du lot suivant | Actif remis (lien vérifié) | IT, ADMIN | Rapproché par n° d'inventaire |
| `Observations` | Observations | Déjà couvert | Livré, recette du lot suivant | Observation du mouvement | IT, ADMIN | Reprise |
| `RecuParEmail` | RecuParEmail | Couvert autrement, sans perte | Livré, recette du lot suivant | Opérateur de la restitution | IT, ADMIN | Conservé dans l'observation |
| `RecuParNom` | RecuParNom | Couvert autrement, sans perte | Livré, recette du lot suivant | Opérateur de la restitution | IT, ADMIN | Conservé dans l'observation |
| `Title` | Titre | Remplacé par une décision métier | Livré, recette du lot suivant | Référence du mouvement MVT-00000 | — | Remplacé par la référence de reprise |
| `TypeObjet` | TypeObjet | Déjà couvert | Livré, recette du lot suivant | Nature de la ligne (actif, SIM, accessoire) | IT, ADMIN | Repris |

## AffectationsVehicules (14 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `DateRemise` | DateRemise | Déjà couvert | Livré, recette du lot suivant | Date de remise | RH, ADMIN | Reprise |
| `DateRestitution` | DateRestitution | Déjà couvert | Livré, recette du lot suivant | Date de restitution | RH, ADMIN | Reprise |
| `Dossier` | Dossier | Remplacé par une décision métier | — | Anciens dossiers non repris | — | Lien non repris |
| `Employe` | Employe | Déjà couvert | Livré, recette du lot suivant | Salarié | RH, ADMIN | Par ancien ID ou titre repris |
| `EtatDepart` | EtatDepart | Déjà couvert | Livré, recette du lot suivant | État à la remise | RH, ADMIN | Repris |
| `KmDepart` | KmDepart | Déjà couvert | Livré, recette du lot suivant | Kilométrage à la remise | RH, ADMIN | Repris |
| `KmRetour` | KmRetour | Déjà couvert | Livré, recette du lot suivant | Kilométrage au retour | RH, ADMIN | Repris |
| `ObservationsRetour` | ObservationsRetour | Déjà couvert | Livré, recette du lot suivant | Observations au retour | RH, ADMIN | Reprises |
| `RecuParEmail` | RecuParEmail | Couvert autrement, sans perte | Livré, recette du lot suivant | Opérateur du retour | RH, ADMIN | Conservé dans l'observation |
| `RecuParNom` | RecuParNom | Couvert autrement, sans perte | Livré, recette du lot suivant | Opérateur du retour | RH, ADMIN | Conservé dans l'observation |
| `RemisParEmail` | RemisParEmail | Couvert autrement, sans perte | Livré, recette du lot suivant | Opérateur de la remise | RH, ADMIN | Conservé dans l'observation |
| `RemisParNom` | RemisParNom | Couvert autrement, sans perte | Livré, recette du lot suivant | Opérateur de la remise | RH, ADMIN | Conservé dans l'observation |
| `Title` | Titre | Remplacé par une décision métier | Livré, recette du lot suivant | Référence VEH-00000 | — | Remplacé |
| `Vehicule` | Vehicule | Déjà couvert | Livré, recette du lot suivant | Véhicule du mouvement | RH, ADMIN | Rapproché par immatriculation |

## CatalogueFormations (7 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `Categorie` | Categorie | Déjà couvert | 1re livraison | Catalogue : catégorie | FORMATION, ADMIN | Reprise |
| `Description` | Description | Déjà couvert | 1re livraison | Catalogue : description | FORMATION, ADMIN | Reprise |
| `DureeHeures` | DureeHeures | Déjà couvert | 1re livraison | Catalogue : durée | FORMATION, ADMIN | Reprise |
| `FormateurEmail` | FormateurEmail | Couvert autrement, sans perte | 1re livraison | Intervenant par défaut (compte habilité FORMATION) | FORMATION, ADMIN | Relié si le compte existe et est habilité |
| `FormateurNom` | FormateurNom | Partiellement couvert | 1re livraison | Intervenant par défaut | FORMATION, ADMIN | Sans compte, le nom n'est pas repris : créer le compte |
| `Support` | Support | Déjà couvert | 1re livraison | Catalogue : lien vers le support | FORMATION, ADMIN | Repris (adresse du lien) |
| `Title` | Intitule | Déjà couvert | 1re livraison | Formations → catalogue : intitulé | FORMATION, ADMIN | Repris |

## Dossiers (34 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `BesoinCarteVisite` | BesoinCarteVisite | Déjà couvert | 1re livraison | Besoin de cartes (tâche Graphiste) | RH, ADMIN | Non repris |
| `BesoinEmailPro` | BesoinEmailPro | Déjà couvert | 1re livraison | Besoin d'adresse professionnelle (Oui / Non / À préciser) | RH, ADMIN | Non repris |
| `BesoinIntellix` | BesoinIntellix | Déjà couvert | 1re livraison | Besoin Intelix | RH, ADMIN | Non repris |
| `BesoinLigneMobile` | BesoinLigneMobile | Déjà couvert | 1re livraison | Besoin SIM / ligne (distinct du téléphone) | RH, ADMIN | Non repris |
| `BesoinPCPortable` | BesoinPCPortable | Couvert autrement, sans perte | 1re livraison | Besoin PC + préférence fixe / portable / sans préférence | RH, ADMIN | Non repris |
| `BesoinsValides` | BesoinsValides | Remplacé par une décision métier | — | Pas d'étape « Valider les besoins » : IT et Formation voient l'annonce immédiatement | — | Non repris |
| `BesoinsValidesParEmail` | BesoinsValidesParEmail | Remplacé par une décision métier | — | Idem (journal : auteur de chaque révision de besoin) | — | Non repris |
| `BesoinVehicule` | BesoinVehicule | Déjà couvert | 1re livraison | Besoin véhicule (tâche RH) | RH, ADMIN | Non repris |
| `BesoinXena` | BesoinXena | Déjà couvert | 1re livraison | Besoin Xena | RH, ADMIN | Non repris |
| `CleCreation` | CleCreation | Couvert autrement, sans perte | 1re livraison | Clé d'idempotence de la commande (aucune double création) | — | Non repris |
| `CommentaireBesoins` | CommentaireBesoins | Couvert autrement, sans perte | 1re livraison | Précision par besoin (suivable besoin par besoin) | RH, ADMIN | Non repris |
| `Commentaires` | Commentaires | Déjà couvert | 1re livraison | Note RH du dossier (RH et ADMIN seulement) | RH, ADMIN | Non repris |
| `DateEffet` | DateEffet | Déjà couvert | 1re livraison | Date d'effet du dossier | RH, ADMIN | Non repris |
| `DateValidationBesoins` | DateValidationBesoins | Remplacé par une décision métier | — | Idem (journal horodaté des révisions) | — | Non repris |
| `DemandeurEmail` | DemandeurEmail | Couvert autrement, sans perte | 1re livraison | Auteur du dossier (compte nominatif) | — | Non repris |
| `DemandeurNom` | DemandeurNom | Couvert autrement, sans perte | 1re livraison | Auteur du dossier (compte nominatif) | — | Non repris |
| `DepartementCible` | DepartementCible | Déjà couvert | 1re livraison | Département à l'arrivée / département cible (mutation) | RH, ADMIN | Non repris |
| `Employe` | Employe | Déjà couvert | 1re livraison | Dossier rattaché à la fiche (jamais de seconde fiche) | RH, ADMIN | Non repris |
| `FonctionCible` | FonctionCible | Déjà couvert | 1re livraison | Poste prévu (arrivée) / poste cible (mutation) | RH, ADMIN | Non repris |
| `FormationsRequises` | FormationsRequises | Déjà couvert | 1re livraison | Sessions de formation du dossier | RH (demande), FORMATION (réalisation) | Non repris |
| `ManagerCible` | ManagerCible | Déjà couvert | 1re livraison | Manager à l'arrivée / manager cible (mutation) | RH, ADMIN | Non repris |
| `MaterielRequis` | MaterielRequis | Déjà couvert | 1re livraison | Besoins d'équipement distincts avec quantités (PC, écran, imprimante…) | RH, ADMIN | Non repris |
| `MessageriePrevue` | MessageriePrevue | Remplacé par une décision métier | — | Plateforme choisie par l'IT au moment de créer l'adresse | — | Non repris |
| `ProfilCible` | ProfilCible | Déjà couvert | 1re livraison | Profil choisi à l'annonce / profil cible (mutation) | RH, ADMIN | Non repris |
| `Progression` | Progression | Couvert autrement, sans perte | 1re livraison | Calculée : actions ouvertes, quantités remises | — | Non repris |
| `SiteCible` | SiteCible | Déjà couvert | 1re livraison | Site prévu (arrivée) / site cible (mutation) | RH, ADMIN | Non repris |
| `SituationApresJSON` | SituationApresJSON | Couvert autrement, sans perte | Livré, recette du lot suivant | Mutation : cible et écarts « conserver, ajouter, reprendre, vérifier » | RH, ADMIN | Non repris |
| `SituationAvantJSON` | SituationAvantJSON | Couvert autrement, sans perte | Livré, recette du lot suivant | Mutation : photographie figée de la situation | RH, ADMIN | Non repris |
| `Statut` | Statut | Couvert autrement, sans perte | 1re livraison | Ouvert, clos, annulé ; attente portée par la tâche | RH, ADMIN | Non repris ; « Brouillon » remplacé par l'annonce immédiate |
| `TachesEnRetard` | TachesEnRetard | Couvert autrement, sans perte | 1re livraison | Calculé : états des lieux, tâches en retard | — | Non repris |
| `Title` | Reference | Couvert autrement, sans perte | 1re livraison | Référence lisible ONB-, DEM-, MUT-, OFF-00000 | — | Dossiers non repris (voir décision) |
| `TypeDossier` | TypeDossier | Déjà couvert | 1re livraison | Type du dossier (arrivée, mutation, départ, demande) | RH, ADMIN | Non repris |
| `VersionProcessus` | VersionProcessus | Remplacé par une décision métier | — | Version des migrations de l'application | — | Non repris |
| `WilayasCibles` | WilayasCibles | Partiellement couvert | 1re livraison | Wilayas à l'arrivée : oui ; wilayas cibles d'une mutation : à livrer | RH, ADMIN | Non repris |

## Formations (10 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `DatePrevue` | DatePrevue | Déjà couvert | Livré, recette du lot suivant | Date prévue | FORMATION | Reprise |
| `DateRealisee` | DateRealisee | Déjà couvert | Livré, recette du lot suivant | Date réelle | FORMATION | Reprise |
| `Dossier` | Dossier | Remplacé par une décision métier | — | Anciens dossiers non repris | — | Session reprise sans dossier, sans tâche |
| `Employe` | Employe | Déjà couvert | Livré, recette du lot suivant | Session : salarié | FORMATION | Par ancien ID ou titre repris |
| `Evaluation` | Evaluation | Déjà couvert | 1re livraison | Évaluation : acquis, à revoir | FORMATION | Reprise |
| `FormateurEmail` | FormateurEmail | Couvert autrement, sans perte | Livré, recette du lot suivant | Intervenant (compte FORMATION) | FORMATION | Relié si le compte existe |
| `FormateurNom` | FormateurNom | Partiellement couvert | Livré, recette du lot suivant | Intervenant | FORMATION | Sans compte, non relié |
| `Formation` | Formation | Déjà couvert | Livré, recette du lot suivant | Session : formation du catalogue | FORMATION | Rapprochée par intitulé |
| `Statut` | Statut | Déjà couvert | Livré, recette du lot suivant | À planifier, planifiée, réalisée, reportée, dispensée | FORMATION | Repris ; jamais « réalisée » sans source |
| `Title` | Titre | Remplacé par une décision métier | — | Session identifiée par formation + salarié | — | Non repris |

## LignesMobiles (7 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `AttribueA` | AttribueA | Couvert autrement, sans perte | Livré, recette du lot suivant | Titulaire déduit de la garde de la SIM | IT, ADMIN | Garde reprise si l'ICCID existe |
| `Forfait` | Forfait | Déjà couvert | Livré, recette du lot suivant | Ligne : forfait (filtrable) | IT, ADMIN | Repris |
| `ICCID` | ICCID | Couvert autrement, sans perte | Livré, recette du lot suivant | Carte SIM liée à la ligne | IT, ADMIN | Reprise ; jamais inventée si absente |
| `Operateur` | Operateur | Déjà couvert | Livré, recette du lot suivant | Ligne : opérateur | IT, ADMIN | Repris |
| `Statut` | Statut | Couvert autrement, sans perte | Livré, recette du lot suivant | État de la ligne ; « Attribuée » déduit de la SIM détenue | IT, ADMIN | Reprise |
| `Title` | Numero | Déjà couvert | Livré, recette du lot suivant | Parc IT → SIM et lignes : numéro | IT, ADMIN | Repris (clé) |
| `TypeLigne` | TypeLigne | Déjà couvert | Livré, recette du lot suivant | Ligne : Voix, Data, Voix + Data | IT, ADMIN | Repris |

## MaterielIT (12 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `AttribueA` | AttribueA | Couvert autrement, sans perte | Livré, recette du lot suivant | Détenteur déduit de l'affectation active | IT, ADMIN | Garde reprise par un mouvement « reprise » (sans décharge) |
| `Categorie` | Categorie | Déjà couvert | Livré, recette du lot suivant | Parc IT → type (15 catégories) | IT, ADMIN | Reprise ; catégorie inconnue rejetée |
| `DateAchat` | DateAchat | Déjà couvert | Livré, recette du lot suivant | Fiche actif | IT, ADMIN | Reprise |
| `Etat` | Etat | Déjà couvert | Livré, recette du lot suivant | État physique (neuf, bon, usé, HS), distinct du statut | IT, ADMIN | Repris |
| `FinGarantie` | FinGarantie | Déjà couvert | Livré, recette du lot suivant | Fiche actif | IT, ADMIN | Reprise |
| `Marque` | Marque | Déjà couvert | Livré, recette du lot suivant | Fiche actif | IT, ADMIN | Reprise |
| `Modele` | Modele | Déjà couvert | Livré, recette du lot suivant | Fiche actif | IT, ADMIN | Repris |
| `NomIntune` | NomIntune | Déjà couvert | Livré, recette du lot suivant | Fiche actif | IT, ADMIN | Repris |
| `NumSerie` | NumSerie | Déjà couvert | Livré, recette du lot suivant | Fiche actif | IT, ADMIN | Repris |
| `Observations` | Observations | Déjà couvert | Livré, recette du lot suivant | Notes de l'actif | IT, ADMIN | Reprises |
| `Statut` | Statut | Couvert autrement, sans perte | Livré, recette du lot suivant | Statut d'usage ; « Attribué » déduit de la garde réelle | IT, ADMIN | « Attribué » reconstitué par AttribueA ou Affectations |
| `Title` | NumInventaire | Déjà couvert | Livré, recette du lot suivant | Parc IT → n° d'inventaire | IT, ADMIN | Repris (clé) |

## ModelesTaches (8 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `ActionTache` | ActionTache | Couvert autrement, sans perte | 1re livraison | Action déduite du besoin (remise, compte, carte, véhicule, restitution, coupure) | — | Non repris |
| `Condition` | Condition | Couvert autrement, sans perte | 1re livraison | Seul « Oui » crée une tâche (À préciser n'invente rien) | — | Non repris |
| `DelaiJours` | DelaiJours | À livrer | — | Échéance = date d'effet ; délai paramétrable à livrer | — | Non repris |
| `Equipe` | Equipe | Déjà couvert | 1re livraison | Équipe responsable : routage besoin → équipe | — | Non repris |
| `Instructions` | Instructions | À livrer | — | Instructions par modèle de tâche : à livrer | — | Non repris |
| `OrdreTache` | OrdreTache | Remplacé par une décision métier | — | Pas d'ordre imposé : Formation n'attend pas l'IT ; ordre d'affichage à livrer | — | Non repris |
| `Title` | Titre | Partiellement couvert | 1re livraison | Intitulé de tâche généré par besoin (non paramétrable) | — | Non repris : à paramétrer (feuille de route) |
| `TypeDossier` | TypeDossier | Partiellement couvert | 1re livraison | Tâches générées selon le type de dossier | — | Non repris |

## Personnel (25 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `BesoinEmailPro` | BesoinEmailPro | Remplacé par une décision métier | — | Le besoin appartient au dossier (ONB, DEM), pas à la fiche | — | Non repris comme besoin ; l'adresse réelle vient de EmailPro |
| `CarteVisite` | CarteVisite | Point à arbitrer | — | Une carte se demande par une demande ponctuelle (Graphiste) | — | Non reprise : demandée, produite ou détenue ? À trancher |
| `DateActivationM365` | DateActivationM365 | Déjà couvert | 1re livraison | Date de vérification M365 | IT, ADMIN | Repris |
| `DateEntree` | DateEntree | Déjà couvert | 1re livraison | Arrivée prévue (à venir) ou présence réelle confirmée (actif) | RH, ADMIN | Repris ; une date réelle confirmée n'est jamais réécrite par import |
| `DateSortie` | DateSortie | Déjà couvert | 1re livraison | Date de sortie (sorti) ou sortie prévue (en départ) | RH, ADMIN | Repris |
| `Departement` | Departement | Déjà couvert | 1re livraison | Fiche → situation ; filtre de la liste | RH, ADMIN | Repris (texte de la liste fermée historique) |
| `EmailPro` | EmailPro | Couvert autrement, sans perte | 1re livraison | Fiche → comptes et accès (adresse réellement créée) | IT, ADMIN | Reprise comme compte e-mail si la plateforme est connue (cPanel ou M365), sinon signalée |
| `Fonction` | Fonction | Déjà couvert | 1re livraison | Fiche → poste réel (distinct du profil) | RH, ADMIN | Repris ; vide → « À préciser » |
| `LignePrincipale` | LignePrincipale | Déjà couvert | 1re livraison | Fiche → lignes mobiles, marque « principale » | RH, IT, ADMIN (parmi les lignes réellement détenues) | Appliquée seulement si la SIM de cette ligne est détenue (reprendre LignesMobiles d'abord) |
| `M365Operationnel` | M365Operationnel | Déjà couvert | 1re livraison | Fiche → « Compte Microsoft 365 opérationnel — vérifié » | IT, ADMIN | Repris, indépendant de l'adresse |
| `Manager` | Manager | Déjà couvert | 1re livraison | Fiche → manager choisi parmi les salariés (lien cliquable) | RH, ADMIN | Par ancien ID, sinon titre repris unique, sinon e-mail pro unique ; sinon « à rattacher » |
| `ManagerEmail` | ManagerEmail | Couvert autrement, sans perte | 1re livraison | Sert uniquement à retrouver la fiche du manager | — | Rapprochement sur l'adresse professionnelle consignée, seulement si elle est unique |
| `ManagerNom` | ManagerNom | Couvert autrement, sans perte | 1re livraison | Affiché « à rattacher » tant qu'aucune fiche n'est reliée | RH, ADMIN | Conservé comme texte d'attente, jamais utilisé seul pour relier |
| `Matricule` | Matricule | Déjà couvert | 1re livraison | Fiche → identité (texte, « 00042 » conservé) | RH, ADMIN | Repris ; sert à l'identification, jamais le nom |
| `NomFamille` | NomFamille | Déjà couvert | 1re livraison | Fiche → identité ; annonce d'arrivée | RH, ADMIN | Repris tel quel |
| `PlateformeMessagerie` | PlateformeMessagerie | Couvert autrement, sans perte | 1re livraison | Plateforme du compte e-mail consigné (choisie par l'IT) | IT, ADMIN | « Inconnu » ou « Aucune » : pas de compte inventé |
| `PosteFixe` | PosteFixe | Point à arbitrer | 1re livraison | Fiche → « Poste fixe » (ancien champ, sens à confirmer) | RH, ADMIN | Repris tel quel, sans interprétation |
| `Prenom` | Prenom | Déjà couvert | 1re livraison | Fiche → identité ; annonce d'arrivée | RH, ADMIN | Repris tel quel (accents et arabe conservés) |
| `ProfilPoste` | ProfilPoste | Déjà couvert | 1re livraison | Fiche → profil de poste (référentiel) ; filtre de la liste | RH, ADMIN | Rapproché par intitulé après reprise de ProfilsPoste |
| `RemarqueImport` | RemarqueImport | Déjà couvert | 1re livraison | Fiche → remarque de reprise (RH et ADMIN seulement) | RH, ADMIN | Reprise comme note |
| `Site` | Site | Déjà couvert | 1re livraison | Fiche → situation : site de rattachement administratif | RH, ADMIN (mutation pour un changement) | Libellé rapproché du référentiel des sites ; site inconnu = ligne rejetée |
| `Statut` | Statut | Déjà couvert | 1re livraison | Statut de la fiche : à venir, actif, en départ, sorti | RH, ADMIN (présence confirmée, en départ) ; sortie par le dossier OFF | « En départ » repris comme présent + indicateur « en départ » |
| `Title` | NomComplet | Couvert autrement, sans perte | 1re livraison | Calculé (prénom + nom) sur la fiche et dans la liste | — | Sert de lien pour les autres listes reprises (titre de la fiche reprise) |
| `TypeContrat` | TypeContrat | Déjà couvert | 1re livraison | Fiche → informations RH (RH et ADMIN seulement) | RH, ADMIN | CDI, CDD, Stage, Intérim, Apprentissage, Consultant |
| `Wilayas` | Wilayas | Déjà couvert | 1re livraison | Fiche → wilayas couvertes (aucune, une ou plusieurs) ; filtre de la liste | RH, ADMIN | Rapprochées par libellé ou code après reprise de Wilayas |

## ProfilsPoste (15 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `AccesACreer` | AccesACreer | Déjà couvert | 1re livraison | Profil → autres accès à préciser | RH, ADMIN | Repris |
| `BesoinEmailPro` | BesoinEmailPro | Déjà couvert | 1re livraison | Profil → besoin d'adresse professionnelle | RH, ADMIN | Repris |
| `BesoinIntellix` | BesoinIntellix | Déjà couvert | 1re livraison | Profil → besoin Intelix | RH, ADMIN | Repris |
| `BesoinXena` | BesoinXena | Déjà couvert | 1re livraison | Profil → besoin Xena | RH, ADMIN | Repris |
| `CarteVisite` | CarteVisite | Déjà couvert | 1re livraison | Profil → besoin de cartes de visite | RH, ADMIN | Repris |
| `Departement` | Departement | Déjà couvert | 1re livraison | Profil → département | RH, ADMIN | Repris |
| `Description` | Description | Déjà couvert | 1re livraison | Profil → description | RH, ADMIN | Repris |
| `FonctionsTypes` | FonctionsTypes | Déjà couvert | 1re livraison | Profil → fonctions types | RH, ADMIN | Repris |
| `FormationsRequises` | FormationsRequises | Déjà couvert | 1re livraison | Profil → formations recommandées (entrées du catalogue) | RH, ADMIN | Rapprochées par intitulé après reprise du catalogue |
| `LigneMobile` | LigneMobile | Déjà couvert | 1re livraison | Profil → besoin SIM / ligne | RH, ADMIN | Repris |
| `MaterielStandard` | MaterielStandard | Déjà couvert | 1re livraison | Profil → besoins suggérés : PC (préférence), écran, tablette, imprimante, accessoire, téléphone, avec quantités | RH, ADMIN | Chaque matériel devient un besoin distinct ; valeur inconnue signalée |
| `MessageriePrevue` | MessageriePrevue | Remplacé par une décision métier | — | L'IT choisit la plateforme ; le profil n'en impose pas | — | Non reprise (signalée à l'aperçu) |
| `Terrain` | Terrain | Déjà couvert | 1re livraison | Profil → poste terrain | RH, ADMIN | Repris |
| `Title` | Intitule | Déjà couvert | 1re livraison | Administration → Profils de poste : intitulé | RH, ADMIN | Repris |
| `Vehicule` | Vehicule | Déjà couvert | 1re livraison | Profil → besoin véhicule (orienté vers le RH) | RH, ADMIN | Repris |

## Taches (15 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `ActionTache` | ActionTache | Couvert autrement, sans perte | 1re livraison | Besoin ou contrôle de sortie lié | — | Non repris |
| `AssigneEmail` | AssigneEmail | Couvert autrement, sans perte | 1re livraison | Responsable (compte nominatif) | Équipe | Non repris |
| `AssigneNom` | AssigneNom | Couvert autrement, sans perte | 1re livraison | Responsable : prise en charge atomique | Équipe | Non repris |
| `Commentaire` | Commentaire | Couvert autrement, sans perte | 1re livraison | Observation de fin, raison d'attente | Équipe | Non repris |
| `DateRealisation` | DateRealisation | Déjà couvert | 1re livraison | Date de fin, avec la preuve (remis, déjà détenu, exception, réalisé) | Équipe | Non repris |
| `Dossier` | Dossier | Déjà couvert | 1re livraison | Tâche rattachée à son dossier | — | Non repris |
| `Echeance` | Echeance | Déjà couvert | 1re livraison | Échéance de la tâche | — | Non repris |
| `Employe` | Employe | Déjà couvert | 1re livraison | Via le dossier | — | Non repris |
| `Equipe` | Equipe | Déjà couvert | 1re livraison | Équipe de la tâche (file « Mon travail ») | — | Non repris |
| `Instructions` | Instructions | À livrer | — | Instructions venant du modèle : à livrer | — | Non repris |
| `OrdreTache` | OrdreTache | Remplacé par une décision métier | — | Aucun ordre imposé entre équipes | — | Non repris |
| `RealiseeParEmail` | RealiseeParEmail | Déjà couvert | 1re livraison | Auteur de la fin de tâche | — | Non repris |
| `RealiseeParNom` | RealiseeParNom | Déjà couvert | 1re livraison | Auteur de la fin de tâche | — | Non repris |
| `Statut` | Statut | Couvert autrement, sans perte | 1re livraison | À prendre, en cours, en attente, terminée, annulée ; « N/A » = exception motivée | Équipe responsable | Non repris |
| `Title` | Titre | Déjà couvert | 1re livraison | Tâche du dossier | — | Non reprises (historique consultable dans l'ancienne application) |

## UtilisateursApp (5 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `Actif` | Actif | Déjà couvert | 1re livraison | Compte actif ou désactivé | ADMIN | Repris ; aucun mot de passe : lien d'activation à générer |
| `Email` | Email | Déjà couvert | 1re livraison | Identifiant de connexion et adresse des avis | ADMIN | Repris (clé) |
| `Equipe` | Equipe | Partiellement couvert | 1re livraison | Rôles : RH, IT, Graphiste, Formateur → FORMATION | ADMIN | Manager et Moyens généraux : aucun rôle accordé, arbitrage |
| `Role` | Role | Remplacé par une décision métier | 1re livraison | « Admin » → ADMIN ; « Responsable » et « Utilisateur » ne donnent aucun droit | ADMIN | Seul Admin est repris |
| `Title` | NomPrenom | Déjà couvert | 1re livraison | Administration → Utilisateurs : nom | ADMIN | Repris |

## Vehicules (11 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `AttribueA` | AttribueA | Couvert autrement, sans perte | Livré, recette du lot suivant | Titulaire déduit de l'affectation active | RH, ADMIN | Garde reprise |
| `DateFinLocation` | DateFinLocation | Déjà couvert | Livré, recette du lot suivant | Fin du contrat ; alerte avant échéance | RH, ADMIN | Reprise |
| `EcheanceAssurance` | EcheanceAssurance | Déjà couvert | Livré, recette du lot suivant | Pièce « assurance » avec échéance | RH, ADMIN | Reprise comme pièce |
| `EcheanceCT` | EcheanceCT | Déjà couvert | Livré, recette du lot suivant | Pièce « contrôle technique » avec échéance | RH, ADMIN | Reprise comme pièce |
| `Kilometrage` | Kilometrage | Déjà couvert | Livré, recette du lot suivant | Compteur (contrôlé à chaque mouvement) | RH, ADMIN | Repris |
| `Loueur` | Loueur | Déjà couvert | Livré, recette du lot suivant | Contrat de location | RH, ADMIN | Repris |
| `Marque` | Marque | Déjà couvert | Livré, recette du lot suivant | Fiche véhicule | RH, ADMIN | Reprise |
| `Modele` | Modele | Déjà couvert | Livré, recette du lot suivant | Fiche véhicule | RH, ADMIN | Repris |
| `Statut` | Statut | Couvert autrement, sans perte | Livré, recette du lot suivant | Disponible, affecté, immobilisé, rendu au loueur, réformé | RH, ADMIN | « Attribué » déduit de la garde |
| `Title` | Immatriculation | Déjà couvert | Livré, recette du lot suivant | Véhicules : immatriculation | RH, ADMIN | Reprise (clé) |
| `TypeVehicule` | TypeVehicule | Déjà couvert | Livré, recette du lot suivant | Propriété ou location | RH, ADMIN | Repris |

## Wilayas (7 champs)

| Champ | Libellé | Situation | Disponible | Où le saisir / le consulter | Qui modifie | Reprise |
|---|---|---|---|---|---|---|
| `CodeWilaya` | CodeWilaya | Déjà couvert | 1re livraison | Administration → Profils et wilayas : code | ADMIN | Clé de reprise |
| `Latitude` | Latitude | Déjà couvert | 1re livraison | Coordonnées (si présentes à la source) | ADMIN | Reprises ; jamais complétées automatiquement |
| `Longitude` | Longitude | Déjà couvert | 1re livraison | Coordonnées (si présentes à la source) | ADMIN | Reprises |
| `OrdreRegion` | OrdreRegion | Déjà couvert | 1re livraison | Ordre d'affichage | ADMIN | Repris |
| `Region` | Region | Déjà couvert | 1re livraison | Région de la wilaya (jamais déduite du site d'un salarié) | ADMIN | Reprise |
| `Title` | Libelle | Couvert autrement, sans perte | 1re livraison | Libellé affiché (code + nom) | ADMIN | Utilisé si « Wilaya » est vide |
| `Wilaya` | Wilaya | Déjà couvert | 1re livraison | Libellé de la wilaya | ADMIN | Repris |

## Décisions sur les anciens champs ambigus

- **ManagerNom / ManagerEmail** : servent à retrouver la fiche du manager (ancien ID, titre repris, puis adresse professionnelle unique). Jamais de rattachement sur le nom seul : sinon la fiche affiche « à rattacher ».
- **RemarqueImport** : reprise comme « remarque de reprise », visible du RH et de l'ADMIN seulement.
- **CarteVisite (Personnel)** : non reprise. « Carte demandée », « carte produite » et « carte détenue » ne sont pas synonymes ; une carte se demande par une demande ponctuelle Graphiste. *À arbitrer* : faut-il reprendre l'information comme « carte déjà détenue » ?
- **BesoinEmailPro (Personnel)** : non repris comme besoin. Besoin d'adresse (dossier), adresse créée (compte consigné), plateforme (choix IT) et M365 vérifié (indicateur) sont quatre informations distinctes, toutes gérées ailleurs.
- **PosteFixe** : repris tel quel dans un champ neutre, affiché « sens à confirmer ». *À arbitrer* : ordinateur fixe ou téléphone fixe ?
- **MessageriePrevue (profil, dossier)** : remplacée par le choix de l'IT au moment de créer l'adresse.
- **Validation des besoins (Dossiers)** : supprimée ; IT et Formation voient l'annonce dès sa création.
- **UtilisateursApp « Manager », « Moyens généraux », « Responsable »** : n'accordent aucun droit. *À arbitrer* au cas par cas pour les personnes concernées.
