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
