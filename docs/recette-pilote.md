# Fiche de recette du pilote (à suivre soi-même)

Durée : environ 45 minutes. Instance de **test** uniquement, jamais la production.

**Préparation**
1. Base vide.
2. `MAIL_MODE=journal` (valeur par défaut) ; archivage désactivé.
3. `seed-demo` avec un mot de passe de démonstration.
4. Démarrer `serve` et `worker`.
5. Comptes fictifs : `rh.demo`, `admin.demo`, `it.demo`, `formation.demo`, `graphiste.demo`, `it.formation.demo` (`@exemple.test`).

Cochez chaque ligne : **OK** si le résultat attendu est obtenu, sinon notez ce que vous voyez.

## A. Arrivée

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| A1 | RH | « Annoncer une arrivée » : prénom, nom, poste, site, date ; rien d'autre | « ONB-… créé », « aucune action précise : annonce seule ». IT et Formation voient le dossier, sans tâche |
| A2 | ADMIN | Même chose avec un autre salarié | Même résultat |
| A3 | ADMIN | Administration → Profils et wilayas → « + Profil » : PC (préférence fixe), SIM, adresse email, Xena, deux formations | Profil listé : « PC (fixe), SIM / ligne, Adresse email professionnelle, Accès Xena » |
| A4 | RH | Nouvelle arrivée, « Informations facultatives » → ce profil. Xena → **Non** ; Écran → **Oui**, quantité 2 ; décocher une des deux formations | PC « Oui, fixe », email « Oui » **sans choix de plateforme**, mention « suggéré par le profil ». 5 actions créées |
| A5 | ADMIN | Modifier le profil (retirer la SIM, ajouter une tablette) | Le dossier de A4 ne change pas |

## B. Travail des équipes

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| B1 | FORMATION | Formations → la session → « Marquer réalisée ». Animée par : « Une personne sans compte… », saisir un nom | Session « réalisée », « animée par » ce nom, « enregistrée par » vous. Aucun compte créé. Possible **avant** toute remise IT |
| B2 | IT | Fiche → Comptes et accès : adresse email, plateforme **cPanel** | Compte consigné ; la case « Microsoft 365 opérationnel » reste **décochée** |
| B3 | IT | Dossier → tâche PC → « Je prends en charge » → « Remettre le matériel ». Ajouter un **portable**, **un** écran et une SIM | Bouton bloqué tant que « Raison de la substitution » est vide. Une fois le motif saisi : « Remise enregistrée MVT-… » |
| B4 | IT | Imprimer la décharge (PDF) | Exactement 3 lignes : portable, écran, SIM, et le motif de substitution |
| B5 | IT | Dossier | « PC remis », « Portable remis à la place du fixe », « 1 écran sur 2 remis », « SIM remise » |
| B6 | IT | Terminer la tâche écran | Refus : « Écran : 1/2 remis dans ce dossier » |
| B7 | IT | Terminer la tâche SIM, puis la tâche email | Soldées « Remis » et « Réalisé » |

## C. Présence, clôture, demande ultérieure

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| C1 | RH | « Confirmer l'arrivée » alors que l'écran manque | Salarié « Actif » ; dossier toujours ouvert |
| C2 | RH | « Clôturer » | Refus listant « Tâche ouverte : Remettre l'écran » et « Décharge non signée » |
| C3 | IT | Remettre le second écran ; déposer les scans signés des deux décharges ; terminer la tâche écran | « 2 écrans sur 2 remis » ; décharges « Signé (scan déposé) » |
| C4 | RH | « Clôturer » | Dossier « Clos » |
| C5 | RH | Fiche → « Nouvelle demande » → Cartes de visite, dossier d'origine = l'ONB clos | « DEM-… » créé. Seul le Graphiste est avisé (« consigné — mode test »). L'ONB reste **clos** |

## D. Confidentialité

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| D1 | GRAPHISTE | Ouvrir la DEM, puis la fiche | Il voit la carte demandée, l'adresse email et la ligne. Il ne voit **ni** contrat, ni note RH, ni remarque de reprise, ni matricule, ni biens, ni décharges |
| D2 | GRAPHISTE | Coller dans la barre d'adresse `/api/v1/cases/ONB-…` puis `/api/v1/documents/<id>/original` | « 403 » (accès refusé) |
| D3 | IT + FORMATION | Ouvrir l'ONB | Voit les tâches IT **et** la formation |

## E. Excel

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| E1 | RH | Personnel → Exporter. Ouvrir le classeur dans Excel **et** LibreOffice | Matricule « 00042 » ou « 00123 » avec ses zéros ; contrat, profil, manager (matricule et ID) et wilayas « 9 Blida; 16 Alger; … » |
| E2 | RH | Réimporter ce classeur sans le modifier | Toutes les lignes « Inchangée » |
| E3 | RH | Classeur avec deux « Karim Saidi » sans matricule, un manager écrit seulement par son nom, une ligne sans poste ni site, et une wilaya « 99 » | Homonymes « Conflit à arbitrer » ; manager « indiqué par son seul nom : non modifié » ; ligne incomplète et wilaya inconnue « Erreur (rejetée) » |
| E4 | RH | Ancien modèle à 8 colonnes, en changeant seulement un poste | Seul le poste change ; contrat, profil, manager et wilayas sont conservés |
| E5 | RH | Après E2 à E4 | Aucun dossier, aucune tâche, aucun avis créés par l'import |

## F. Salarié déjà présent (reprise SharePoint, ADMIN)

Fichiers fictifs : `docs/recette-fictive/`.
1. Créer d'abord le site « Alger — Siège ».
2. Charger les listes dans l'ordre proposé : Wilayas, CatalogueFormations, UtilisateursApp, ProfilsPoste, Personnel, MaterielIT, LignesMobiles, Affectations, Formations.
3. Pour chaque liste : aperçu, puis « Appliquer ».

| # | Action | Résultat attendu |
|---|---|---|
| F1 | UtilisateursApp (« Manager » seul) | Compte **non créé** : la règle est citée |
| F2 | Fiche de Nadia Cherif | Manager relié, 3 wilayas, profil, contrat CDI, « Poste fixe » « sens à confirmer », remarque de reprise avec la carte de visite « n'atteste ni impression ni remise », ligne principale |
| F3 | Biens détenus | PC-201 et SIM, **une** garde chacun, depuis le 03/05/2022. Observation « Avec sacoche — attribué par … — réf. SharePoint AFF-0045, dossier SharePoint ONB-2022-031 — sans décharge dans l'application » |
| F4 | Formations | « animée par Mme Bensalem (intervenante externe) · enregistrée par <vous> · reprise SharePoint » |
| F5 | Charger `Dossiers.csv` | Refusé (« liste inconnue ») : la demande ouverte est à ressaisir |
| F6 | Mutation de Nadia vers Oran | La comparaison propose « Conserver » pour le PC ; **aucun champ wilaya** |
| F7 | Avis et mails (ADMIN) | Aucun avis lié à la reprise ; aucun envoi réel |
