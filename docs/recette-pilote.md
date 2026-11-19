# Fiche de recette du pilote (à suivre soi-même)

Instance de **test** uniquement, jamais la production. Durée : environ 40 minutes pour la partie 1.

**Préparation**
1. Base vide.
2. Réglages : `MAIL_MODE=journal`, `ARCHIVE_MODE=desactive`, `MODULES_DESACTIVES=pilote`.
3. `seed-demo` avec un mot de passe de démonstration.
4. Démarrer `serve` **et** `worker`.
5. Comptes fictifs : `rh.demo`, `admin.demo`, `it.demo`, `formation.demo`, `graphiste.demo`, `it.formation.demo` (`@exemple.test`).

Données fictives de reprise : `docs/recette-fictive/`. Ordre de chargement : Wilayas, CatalogueFormations, UtilisateursApp, ProfilsPoste, Personnel, MaterielIT, LignesMobiles, Affectations, Formations.

Cochez **OK** ou notez ce que vous voyez.

## Partie 1 — Pilote

### A. Ce qui est visible

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| A1 | RH | Regarder le menu et l'accueil | Menu : Accueil RH, Mon travail, Dossiers, Personnel, Formations. Ni Véhicules, ni États. L'accueil n'a ni départs, ni mutations, ni échéances véhicules |
| A2 | RH | Taper `/vehicules` puis `/etats` dans la barre d'adresse | « Fonction non ouverte » |
| A3 | RH | Ouvrir une fiche salarié | Ni « Mutation », ni « Départ », ni « Correction de poste datée » ; « Nouvelle demande » sans l'option Véhicule |
| A4 | IT | Parc IT | Onglets Actifs, Accessoires, SIM et lignes seulement ; aucun bouton « Imprimer l'étiquette » |
| A5 | IT | Écran de remise : taper un n° d'inventaire puis Entrée | Le bien est reconnu : le scan d'un bien reste disponible |
| A6 | ADMIN | Administration → Avis et mails | Mode « journal » ; avis « consignés », aucun envoyé |

### B. Arrivée avec un profil qui prévoit un véhicule

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| B1 | ADMIN | Créer le profil « Chauffeur-livreur (fictif) » : adresse email + véhicule | Profil enregistré |
| B2 | RH | Annoncer une arrivée avec ce profil | Avertissement « Ce profil prévoit un véhicule : non géré dans l'application pendant le pilote ». Pas de ligne Véhicule. 1 action créée (adresse email) |
| B3 | IT | Consigner l'adresse (cPanel) et terminer la tâche | Tâche « Réalisé » |
| B4 | RH | Clôturer | Dossier « Clos » : aucune tâche véhicule ne bloque |

### C. Carte de visite demandée après clôture

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| C1 | RH | Fiche du salarié de B → Nouvelle demande → Cartes de visite, dossier d'origine = l'ONB clos | « DEM-… » créé ; seul le Graphiste est avisé (« consigné — mode test ») ; l'ONB reste **clos** |
| C2 | GRAPHISTE | Ouvrir la DEM et la fiche | Carte demandée, email et ligne visibles ; ni contrat, ni note RH, ni biens, ni décharges |

### D. Salarié déjà présent (reprise) : adresse non vérifiée et matériel ancien

| # | Qui | Action | Résultat attendu |
|---|---|---|---|
| D1 | ADMIN | Créer le site « Alger — Siège », puis charger les 9 fichiers fictifs dans l'ordre (Administration → Reprise SharePoint) | Personnel : CarteVisite, BesoinEmailPro et « M365 Oui sans date » gardés en texte. Affectations : « Garde déjà reprise… complétée » |
| D2 | IT | Fiche de Nadia Cherif → Comptes et accès | « Adresse reprise — vérification non renseignée » ; « Microsoft 365 opérationnel » **décoché** ; menu « Confirmer… » avec « Actif (vérifié) » |
| D3 | IT | Biens détenus | PC-201 : « Justificatif dans SharePoint : pièce jointe de l'élément n° 701… ». SIM : « ouvrir l'ancien document ». Jamais « signé » |
| D4 | IT | Restitution → cocher PC-201, état « Bon état » → confirmer | « Restitution enregistrée MVT-… » ; la décharge PDF cite la remise d'origine MVT-… |
| D5 | IT | Sur le même écran, « Déposer le scan signé » | Tout de suite : « Signé (scan déposé) » et « Scan enregistré (version 1, …) : décharge signée ». Rien à redéposer |
| D6 | IT | Choisir « Actif (vérifié) » sur l'adresse | « vérifié le » + la date du jour |
| D7 | GRAPHISTE | Coller l'adresse de la fiche de Nadia | Accès refusé (aucune demande ne la concerne) ; aucune référence SharePoint visible |

### E. Rappels du parcours (déjà validés, à rejouer si le temps le permet)

- Arrivée en cinq champs par le RH et par l'ADMIN.
- Profil : Xena retiré, deux écrans, PC fixe de préférence, besoin d'adresse sans plateforme.
- Formation réalisée avant la remise, « animée par » une personne sans compte.
- Portable remis à la place du fixe avec motif, SIM et un seul écran : la décharge liste exactement ces 3 biens et le second écran reste dû.
- Excel : export, réimport « inchangé », homonymes, ancien modèle à 8 colonnes.

## Partie 2 — Modules futurs (hors pilote)

À jouer **plus tard**, sur une autre instance de test, avec `MODULES_DESACTIVES=` (vide) :
- **Mutation** : préparation, comparaison « conserver / ajouter / reprendre », confirmation, application à la date d'effet par le worker, y compris les wilayas cibles (inchangées avant la date d'effet, remplacées à l'application).
- **Départ** : dernier jour et heure de coupure, coupure confirmée par l'IT, sortie, écarts.
- **Véhicules** : remise depuis une tâche RH, pièces, entretien, retour au loueur, alertes.
- **Logiciels** : sièges, renouvellement, alerte IT.
- **Étiquettes et inventaire QR** : impression d'une planche, campagne, doublon, écarts, avec la douchette réelle.
- **États des lieux** et leur export.
