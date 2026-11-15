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
| RP | Sur une **VM de test distincte de la production** : `backup.sh`, puis `restore-isole.sh` (projet `cellulo-restore`, volumes neufs). | Dossier, rôles (dont IT + FORMATION), empreintes du PDF et du scan identiques ; empreinte du certificat racine Caddy identique avant/après ; mode d'envoi « journal ». Refus sur cible non vide ou de production ; arrêt avant démarrage en cas d'échec. |

## État au 15/11/2026 (branche `lot3-complet`)

Légende : **auto** = test automatisé exécuté et réussi (PostgreSQL 17 réel) ; **nav** = vérifié dans un navigateur sur une instance isolée avec données fictives ; **site** = reste à valider sur site.

| ID | Implémenté | Testé auto | Vérifié nav | À valider sur site |
|---|---|---|---|---|
| R01 | oui | oui (`parcours::r01`, `lot1::avis_honnetes`) | oui | envoi Exchange réel vers une boîte de test |
| R02 | oui | oui (`parcours::r02_r06`, `lot1::preuve_de_remise`) | oui : remise portable + SIM, motif, contenu du PDF relu | impression A4 papier, douchette réelle |
| R03 | oui | oui (`parcours::r03`) | partiel : session réalisée avant le PC ; la prise simultanée n'est testée qu'automatiquement | — |
| R04 | oui | oui (`parcours::r04`) | non | — |
| R05 | oui | oui (`parcours::r05`) | oui : DEM après clôture, vue Graphiste | — |
| R06 | oui | oui (`parcours::r02_r06`) | oui (session du 12/11 matin) | signature et scan réels |
| R07 | oui | oui (`inventaire::r07_campagne_inventaire`) | oui : doublon « Déjà pointé dans ce local », compteurs | lecture d'une planche imprimée à la douchette |
| R08 | oui | oui (`lot2_excel`, 2 tests) | oui : aperçu, décisions, application, export | classeur RH réel, ouverture dans Excel et LibreOffice |
| R09 | oui | oui (`mutation_depart::r09_…`, `mutation_annulee_avant_effet`) | non | — |
| R10 | oui | oui (`mutation_depart::r10_…` : redémarrage simulé, coupure non réputée faite, sortie effective, dossier ouvert) | oui : panneau Départ après passage du worker | — |
| R11 | oui | oui (`vehicules::r11_…`) | oui : remise depuis la tâche RH, tâche soldée « Remis » | — |
| R12 | oui | oui (`logiciels::r12_…` en parallèle, `siege_libere_en_fin_de_periode`, `vehicules::r12_…`) | partiel : liste des logiciels | — |
| R13 | oui : adaptateurs Graph (envoi, archive), reprise après panne, résultat inconnu « à vérifier » | oui (`graph::r13_…`, faux serveur Graph local, signature JWT vérifiée) | — | coupure réelle Exchange/SharePoint pendant une remise |
| T08 | reprise des listes SharePoint | oui (`reprise`, 3 tests) | oui : aperçu MaterielIT, rejet, décision | reprise à blanc avec les exports réels |
| R14 | refus API : oui ; restauration : voir RP | oui (`parcours::r14`, `lot1::graphiste_autorise…`, `lot2_excel::export_droits…`) | oui : 403 en appel direct depuis le navigateur | restauration Docker complète |
| L1 à L4 | oui | oui (`lot1`, 4 tests) | oui (L1 : Formation et Graphiste ; L2 à L4) | — |
| RP | `backup.sh` chiffré (GPG à clé publique, code 3 si non configuré) ; `restore-isole.sh` (VM de test, garde-fous, arrêt avant démarrage) | `test-backup.sh` (21 contrôles) et `test-restore.sh` (22 contrôles) avec faux docker : **oui** ; restauration PostgreSQL + documents : **exécutée et réussie** hors Docker | — | **recette complète sur VM avec vrai Docker : NON EXÉCUTÉE** (sauvegarde → restauration → dossier, rôles, empreintes PDF/scan, certificat racine Caddy avant/après) |

Non validé à ce jour : l'impression des étiquettes QR (rendu PDF non relu à l'écran sur le poste de développement), le rendu des noms arabes dans les PDF (les glyphes sont présents, mais la liaison et le sens de lecture ne sont pas traités) et l'impression papier. La présence d'un en-tête PDF ne vaut pas validation.

## Première livraison : neuf scénarios (état au 15/11/2026)

Légende :
- **nav** : joué dans un navigateur sur une instance locale isolée, avec données fictives ;
- **auto** : test automatisé `crates/server/tests/livraison1.rs`, sur PostgreSQL 17 réel.

| # | Scénario | Résultat |
|---|---|---|
| 1 | Arrivée minimale en cinq champs | **nav** : ONB créé, vu par IT et Formation, aucune tâche. **auto** : `s1_s2_…` |
| 2 | Fiche terrain : Chéraga, manager, trois wilayas | **nav** : manager relié depuis la fiche ; le filtre de chaque wilaya retrouve la personne ; le site reste Chéraga. **auto** : `s1_s2_…` |
| 3 | Profil : Xena retiré, une formation gardée, deux écrans ajoutés ; profil modifié ensuite | **nav** : le dossier reflète les choix et reste identique après modification du profil. **auto** : `s3_a_s7_…` |
| 4 | Formation réalisée avant la remise ; boîte cPanel ; M365 indépendant | **auto** : `s3_a_s7_…` (non rejoué au navigateur) |
| 5 | Portable à la place du fixe, SIM, un écran sur deux | **nav** : motif exigé ; affichage « Portable remis à la place du fixe », « SIM remise », « 1 écran sur 2 remis » ; tâche écran refusée (« 1/2 remis ») ; PDF relu, limité aux trois articles remis. **auto** : `s3_a_s7_…` |
| 6 | Présence confirmée avec équipement manquant ; demande de carte après clôture | **auto** : `s3_a_s7_…` (seul le Graphiste est ciblé ; l'ONB reste clos) |
| 7 | Confidentialité Graphiste ; IT + FORMATION | **auto** : `s3_a_s7_…` (contrat, note RH, commentaire IT et décharges absents du JSON ; accès directs 403) |
| 8 | Excel aller-retour, homonymes, manager ambigu, ligne incomplète, ancien modèle | **auto** : `s8_s9_…` (aucun dossier, aucune tâche, aucun avis) |
| 9 | Fiches et dossiers antérieurs | **auto** : `s8_s9_…` (aucun profil, contrat, présence ni remise inventés : affichés « — ») |

Restent à faire sur site :
- jouer les scénarios 4, 6, 7, 8 et 9 dans le navigateur ;
- ouvrir le classeur exporté dans Excel et dans LibreOffice ;
- imprimer la décharge.
