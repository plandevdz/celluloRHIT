# Cahier UI/UX — application web Celluloplast | version 2.1
Utilisée dans un navigateur (Edge, Chrome, Firefox récents) sur les PC Windows de l’entreprise, à l’adresse interne du serveur. Chaque écran a une URL propre (ex. /dossiers/ONB-00007) que l’on peut mettre en favori ou ouvrir depuis un mail ; les boutons Précédent/Suivant du navigateur fonctionnent. Plusieurs onglets possibles sur la même session.
## Navigation et principe d’écran
Mon travail · Dossiers · Personnel · Parc IT · Véhicules · Formations · Administration. N’afficher que les espaces autorisés. « Mon travail » rassemble toutes les habilitations : une personne IT + FORMATION voit ses deux files, filtrables, avec le même identifiant. Pas de changement de compte. Les états des lieux sont accessibles depuis le module concerné et exportables selon les droits. Référence courte et nom du salarié dans l’en-tête ; jamais un GUID comme titre. Interfaces en français, noms arabes et accents préservés.

| Vue | Information visible | Action principale |
|---|---|---|
| Accueil RH | Arrivées prévues, départs à préparer, signatures en attente, échéances véhicules | Annoncer une arrivée ; ouvrir une fiche |
| Mon travail IT/FORMATION/GRAPHISTE | Équipe, site, salarié, action, échéance, prise en charge et raison d’attente | Je prends en charge, puis ouvrir la tâche |
| Fiche salarié | Situation actuelle, historique du poste, biens détenus, accès et formations selon droits | Nouvelle demande, mutation, départ ; aucun champ de salaire |
| Dossier | Demandes, réalisations, tâches et documents côte à côte ; horodatage | Compléter une demande, lever un blocage ou clôturer |
| Parc IT | Disponibles, réservés, remis, en contrôle, logiciels et sièges, SIM et lignes | Scanner, affecter, restituer, inventorier |
| Véhicules RH | Propriété/location, statut, détenteur, kilométrage, prochaine échéance | Affecter/restituer, consigner entretien, renouveler document |
| Administration | Utilisateurs, rôles multiples, routage, modèles, archivage et mails en échec | Corriger une affectation ; tester avec destinataire de test |

## U1. Arrivée et demandes ponctuelles
Un seul écran court pour l’arrivée : Prénom, Nom, Poste prévu, Site, Date d’arrivée prévue. En dessous, zone repliable « Préciser les besoins maintenant ». Bouton unique « Créer et prévenir les équipes » ; le récapitulatif des destinataires est visible sans deuxième validation. Après succès : « ONB-00007 créé » et état distinct des notifications. Matricule facultatif dans la fiche, avertissement non bloquant pour homonyme. Pas de compte, formation ou bien créé parce qu’un champ est vide.

Options Oui / Non / À préciser ; pour PC, préférence Fixe / Portable / Sans préférence. Email demandé = Oui/Non, seul l’IT voit les champs de plateforme effective. Téléphone et ligne distincts. En ouvrant un dossier : afficher « Demandé : PC fixe de préférence » puis « Remis : portable PC-042 » et l’observation. Ne pas masquer la demande initiale. Une carte oubliée ? Fiche salarié → Nouvelle demande → Carte de visite → Créer : afficher DEM-xxxxx et la file GRAPHISTE, sans ressaisie ni notification générale.

## U2. IT, formation et documents
Technicien : choisir ou scanner un actif libre, voir son état et site ; si substitution, proposer une raison courte préremplissable, sans approbation RH. Préparer puis remettre sont deux actions distinctes. Une remise groupe plusieurs lignes ; une restitution propose les biens encore détenus, sélection ligne par ligne et quantité pour accessoires. Résumé avant confirmation et accès à la preuve imprimable. L’état « signé » nécessite le scan associé, jamais le simple clic Imprimer. Afficher « archivage en attente » si SharePoint indisponible. Le PDF ne doit pas retarder le suivi des autres travaux.
FORMATION : sessions de mon équipe, intervenant, échéance, date réelle et motif de report. Disponible dès l’annonce, indépendamment du compte mail du salarié. GRAPHISTE : coordonnées et éléments de la demande, possibilité de signaler un renseignement manquant sans bloquer le dossier entier.
IT messagerie : adresse réelle, cPanel ou Microsoft 365, « Compte Microsoft 365 opérationnel — vérifié ». Cette case reste indépendante de l’hébergement mail. Aucune saisie de mot de passe du salarié.

## U3. Inventaire QR
Dans Parc IT → Étiquettes, sélectionner les actifs, le format unitaire/planche, la taille et la position de départ ; aperçu et impression à l’échelle réelle avec zone blanche autour du QR. Tester la lecture d’une planche avant tirage. Scanner depuis une fiche ouvre l’actif ; scanner dans une campagne enregistre une observation, jamais une attribution. Écran campagne : site/local en tête, grande zone de scan gardant le focus, dernier résultat en clair et compteurs. Entrée clavier manuelle équivalente. Un doublon affiche « Déjà pointé dans ce local ». Un QR inconnu n’ouvre aucun lien externe. Pour un écart, montrer « attendu » et « observé » ; correction du parc sur action explicite distincte. Rapport final exportable ; un actif non vu est à vérifier.

## U4. Véhicules, logiciels et personnel
Véhicules : filtres Loué / Propre / Tous, disponible, affecté, immobilisé ; onglets Documents, Contrat, Entretien, Mouvements. La restitution d’une location demande si le véhicule revient chez Celluloplast ou au loueur ; le deuxième choix le sort des disponibilités. Les échéances apparaissent avec date/km et responsable.
Logiciels : abonnement et nombre de sièges, titulaires, échéance, disponibilité. Ne jamais matérialiser un siège par une étiquette QR. Une licence perpétuelle n’a pas à recevoir une fausse date de renouvellement.
Personnel : Télécharger modèle, Importer, Exporter. Aperçu avec lignes nouvelles, mises à jour, homonymes, rejets et motifs ; corriger puis prévisualiser à nouveau. Les fiches créées par import ne déclenchent aucune annonce. Une commande explicite distincte démarre un parcours.

## U5. Mutation et démission
Mutation : tableau « Actuel / Cible / Action proposée », date d’effet, choix conserver/ajouter/reprendre/vérifier. Éviter toute ressaisie des biens déjà connus. Distinguer « changement planifié », « poste appliqué » et « actions restantes ». Si la situation a changé depuis la comparaison : ouvrir les écarts, ne pas écraser.
Démission : en tête dernier jour et heure prévue de coupure. Deux panneaux indépendants : « Accès à traiter » et « Biens à restituer » ; afficher compte réel, titulaire de tâche et heure de réalisation. Si le poste n’est pas rendu : action « Signaler un écart » et date de suivi. L’écran peut afficher simultanément « Salarié sorti », « Accès coupés » et « Véhicule non rendu ». En clôture afficher les actions non soldées et, si autorisée, une exception motivée ; aucune case « tout a été fait ».

## U6. Navigateur et états dégradés
Cible 1366×768 et 1280×720, zoom Windows/navigateur 125–150 %, navigation clavier, focus visible, couleurs accompagnées de texte, contrastes contrôlés ; tableaux paginés, filtres persistants (dans l’URL) et recherche référence/matricule/inventaire. En panne réseau, préserver la saisie dans la page (prévenir avant de quitter une page avec saisie non envoyée) ; vérifier l’état de la commande avant de proposer de réessayer. Session expirée : se reconnecter sans perdre la saisie. Ni succès local fictif ni donnée RH conservée dans le stockage du navigateur sur poste partagé ; déconnexion visible. En conflit, montrer l’état actuel et l’auteur habilité ; aucune prise de tâche concurrente silencieuse. Les alertes se limitent à l’équipe compétente. Tests utilisateurs : jouer 05-Recette.md dans Edge et Chrome sur poste Windows, imprimante et douchette physiques (la douchette en mode clavier saisit dans le champ de scan qui garde le focus).
