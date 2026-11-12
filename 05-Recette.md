# Recette fonctionnelle | jeu de test fictif uniquement
Préparer un utilisateur par rôle et un IT + FORMATION ; deux sites et deux locaux, un portable libre, un chargeur, une SIM, un siège logiciel, un véhicule loué et une boîte de test. Couper les intégrations réelles hors pilote autorisé. Les scénarios se jouent dans un navigateur (Edge et Chrome) sur poste Windows, connecté en HTTPS à l’instance Docker de test.

| ID | Action | Preuve attendue |
|---|---|---|
| R01 | RH puis ADMIN annoncent deux recrues différentes avec cinq champs et sans matricule. Rejouer la première requête. | Deux ONB au total. Annonces IT et FORMATION, pas de seconde validation, pas de mail en double à l’utilisateur multi-rôles pour le même avis. |
| R02 | Préférer un fixe, remettre le portable disponible en ajoutant chargeur et SIM. | Préférence intacte ; garde réelle et motif visibles. Un PDF de remise à trois lignes. |
| R03 | Deux techniciens prennent simultanément la même tâche. En parallèle, un IT habilité FORMATION réalise une session. | Un seul propriétaire de la tâche ; formation terminée sans attendre la livraison du PC. |
| R04 | IT crée une adresse cPanel puis confirme séparément l’accès M365 ; deuxième recrue sans adresse demandée. | Une adresse effective et un indicateur indépendant ; aucun compte ni tâche mail imposés à la seconde. |
| R05 | Fermer un ONB soldé, puis demander une carte de visite. | DEM autonome pour le même salarié, seule file GRAPHISTE ; ONB inchangé. |
| R06 | Retourner uniquement portable et chargeur ; joindre scan après impression. | Document de restitution relié à la remise originale, SIM toujours attribuée, impression distincte de signature. |
| R07 | Scanner deux fois un actif au même local, le scanner ailleurs, saisir un code inconnu et laisser un actif non vu. | Doublon neutralisé, écarts explicites, rapport exportable, aucune affectation changée. Réimprimer l’étiquette donne le même code. |
| R08 | Importer deux homonymes sans matricule, une ligne contradictoire et un matricule avec zéros initiaux ; rejouer le lot. | Aperçu obligatoire, ambiguïtés non fusionnées, identifiants préservés, pas de doublon sur rejeu ni notification. |
| R09 | Muter un salarié ayant Xena et un véhicule vers un poste conservant Xena sans véhicule. | Xena conservé ; tâche véhicule RH ; aucun changement de poste anticipé ; à échéance une seule application. Tester report, annulation et conflit de version. |
| R10 | Démission avec matériel non rendu. Simuler un redémarrage à l’heure de coupure. | Tâches échues reprises ; aucun compte réputé coupé sans confirmation IT ; sortie administrative effective ; dossier encore ouvert pour le bien. |
| R11 | Faire restituer un véhicule loué par un salarié sortant à Celluloplast, puis retour au loueur par RH. | Deux mouvements distincts, états et km horodatés ; véhicule non disponible après remise au loueur. |
| R12 | Attribuer le dernier siège logiciel à deux personnes en parallèle, puis en libérer un ; arriver à échéance assurance/vidange. | Une seule attribution initiale ; siège libéré à la restitution ; les alertes véhicule vont au RH et le renouvellement conserve l’historique. |
| R13 | Couper Exchange et SharePoint pendant la remise ; fermer l’onglet du navigateur ; redémarrer le conteneur worker. | Garde conservée, file d’attente visible, archivage repris, aucune fausse signature ou livraison de mail annoncée. |
| R14 | GRAPHISTE tente d’exporter les pièces RH (y compris en appelant directement l’URL de l’API) ; restaurer le serveur (volumes Docker) sur une autre machine en environnement isolé. | Refus côté API ; reprise des données et contrôles sans expédier les notifications de production. |

## Contrôles ajoutés au lot 1 (fiabilisation de l'arrivée)

| ID | Action | Preuve attendue |
|---|---|---|
| L1 | Dossier avec carte de visite, note RH, blocage et observation IT. Le GRAPHISTE ouvre **son** dossier (accès autorisé) et sa fiche salarié ; il appelle aussi directement les URL des comptes, des biens et des décharges. | Il voit sa carte et les coordonnées utiles. Le JSON ne contient ni note RH, ni observation ou blocage IT, ni historique des postes, ni autres dossiers, ni décharges. Les accès directs renvoient 403. Même logique pour FORMATION ; un IT + FORMATION cumule les deux vues. |
| L2 | Recrue présente lundi, formée mardi, PC remis mercredi ; double clic sur « Confirmer l'arrivée » ; correction de date ; clôture d'une autre arrivée sans confirmation. | Comptée active dès lundi, dossier encore ouvert ; un seul événement ; correction motivée et tracée ; la clôture ne confirme rien et la fiche est signalée « présence à confirmer ». |
| L3 | Remise d'une SIM rattachée à la tâche PC ; remise « téléphone + portable » avec préférence fixe ; « déjà détenu » ; « exception ». | La SIM ne solde pas le PC ; motif de substitution exigé quel que soit l'ordre des lignes ; « déjà détenu » vérifié sur une affectation active du bon type ; l'exception exige un motif et ne crée aucune affectation. |
| L4 | Arrivée avec carte de visite sans aucun GRAPHISTE actif, en mode journal. | « Dossier visible par … — aucun e-mail envoyé (mode test) » ; équipe sans destinataire signalée en Administration ; arrivée créée ; « consigné » jamais compté comme « envoyé ». |
| RP | Sauvegarde puis restauration sur base et dossier neufs. | Dossier, rôles (dont IT + FORMATION), empreintes du PDF et du scan identiques ; mode d'envoi « journal ». |

## État au 12/11/2026 (branche `lot1-fiabilisation-lot2-excel`)

Légende : **auto** = test automatisé exécuté et réussi (PostgreSQL 17 réel) ; **nav** = vérifié dans un navigateur sur une instance isolée avec données fictives ; **site** = reste à valider sur site.

| ID | Implémenté | Testé auto | Vérifié nav | À valider sur site |
|---|---|---|---|---|
| R01 | oui | oui (`parcours::r01`, `lot1::avis_honnetes`) | oui | envoi Exchange réel (non livré) |
| R02 | oui | oui (`parcours::r02_r06`, `lot1::preuve_de_remise`) | oui : remise portable + SIM, motif, contenu du PDF relu | impression A4 papier, douchette réelle |
| R03 | oui | oui (`parcours::r03`) | partiel : session réalisée avant le PC ; la prise simultanée n'est testée qu'automatiquement | — |
| R04 | oui | oui (`parcours::r04`) | non | — |
| R05 | oui | oui (`parcours::r05`) | oui : DEM après clôture, vue Graphiste | — |
| R06 | oui | oui (`parcours::r02_r06`) | oui (session du 12/11 matin) | signature et scan réels |
| R07 | **non** (prochaine tranche) | — | — | — |
| R08 | oui | oui (`lot2_excel`, 2 tests) | oui : aperçu, décisions, application, export | classeur RH réel, ouverture dans Excel et LibreOffice |
| R09 à R12 | **non** | — | — | — |
| R13 | partiel : mode journal seul ; Exchange et SharePoint non raccordés | — | — | — |
| R14 | refus API : oui ; restauration : voir RP | oui (`parcours::r14`, `lot1::graphiste_autorise…`, `lot2_excel::export_droits…`) | oui : 403 en appel direct depuis le navigateur | restauration Docker complète |
| L1 à L4 | oui | oui (`lot1`, 4 tests) | oui (L1 : Formation et Graphiste ; L2 à L4) | — |
| RP | `backup.sh` : secrets toujours chiffrés (GPG à clé publique, clé privée hors serveur), code 3 si non configuré | chiffrement/déchiffrement : **oui** (`deploy/tests/test-backup.sh`, secrets fictifs, faux docker) ; restauration PostgreSQL + documents : **exécutée et réussie** hors Docker (base et dossier neufs) | — | restauration complète par volumes Docker (CA comprise) et `backup.sh` contre un vrai Docker : **non exécutées** (Docker indisponible sur le poste de développement) |

Non validé à ce jour : le rendu des noms arabes dans les PDF (les glyphes sont présents, mais la liaison et le sens de lecture ne sont pas traités) et l'impression papier. La présence d'un en-tête PDF ne vaut pas validation.
