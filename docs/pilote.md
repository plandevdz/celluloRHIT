# Pilote : périmètre, salarié déjà présent, écarts, décisions

État au 16/11/2026 (branche `lot3-complet`, PR n° 2 en brouillon).

Toutes les vérifications ont été faites sur une **instance isolée** : données fictives (`@exemple.test`), `MAIL_MODE=journal`, archivage désactivé. Il n'y a eu aucun envoi réel, aucun accès à la production et aucun déploiement.

La fiche à suivre soi-même est dans [recette-pilote.md](recette-pilote.md).

## 1. Que verraient les utilisateurs si cette version était déployée ?

Tous les modules ci-dessous sont **visibles** selon le rôle. Aucun interrupteur ne permet aujourd'hui d'en masquer un.

| Module | Visible par | Joué dans le navigateur | Reste à valider |
|---|---|---|---|
| Arrivée, fiche personnel, profils de poste | RH, ADMIN (annonce) ; IT, Formation, Graphiste (leur part) | **Oui, de bout en bout** : RH et ADMIN, sans profil et avec profil, présence, clôture, demande ultérieure | Impression papier de la décharge |
| Parc IT : actifs, accessoires, SIM, remise et restitution, décharges et scans | IT, ADMIN | **Oui** : remise avec substitution, remise partielle, deux scans, clôture contrôlée. Restitution jouée le 12/11 | Douchette réelle, impression A4 |
| Formations : catalogue, sessions | Formation, RH, ADMIN | **Oui** : session réalisée avant la remise, « animée par » une personne sans compte | — |
| Demandes ponctuelles (carte, matériel) | RH, ADMIN ; l'équipe ciblée | **Oui** : carte demandée après clôture, Graphiste seul prévenu | — |
| Excel du personnel (export et import) | RH, ADMIN | **Oui** : aller-retour sans perte, homonymes, manager par le nom, ligne incomplète, ancien modèle | Ouverture du classeur dans Excel et LibreOffice |
| Reprise SharePoint | ADMIN | **Oui** : 10 listes fictives, salarié déjà présent (section 2) | Reprise à blanc avec **vos** exports réels |
| Véhicules RH | RH, ADMIN (menu « Véhicules ») | Partiel : remise depuis une tâche RH | Contrats, pièces, entretien, retour au loueur, alertes |
| Logiciels et licences | IT, ADMIN (onglet du Parc IT) | Partiel : liste | Sièges, renouvellement, alerte |
| Étiquettes QR et inventaire | IT, ADMIN (onglet et bouton « Imprimer l'étiquette ») | Oui pour la campagne de scan (doublon, compteurs) | Impression des étiquettes, douchette réelle |
| Mutation | RH, ADMIN (bouton sur la fiche) | Préparation et comparaison seulement | Confirmation, application à la date d'effet, suspension. **Les wilayas cibles ne sont pas livrées** |
| Départ | RH, ADMIN (bouton) ; IT (contrôles de sortie) | Panneau après le passage du worker (12/11) | Coupure confirmée par l'IT de bout en bout, sortie, écarts |
| États des lieux | RH, ADMIN, IT (menu « États ») | Non | Tout |
| Envoi Exchange (Graph) | Personne tant que `MAIL_MODE=journal` | Non (testé contre un faux serveur) | Raccordement réel, boîte de test |
| Archivage SharePoint | Personne tant qu'il est désactivé (mention « non raccordé ») | Non (faux serveur) | Raccordement réel |

### Recommandation : le plus petit pilote sans parcours inachevé

Le pilote comprendrait, sur un site, avec données fictives puis reprise à blanc :
- la fiche personnel et les profils de poste ;
- l'arrivée ;
- le parc IT (remise, restitution, décharges et scans) ;
- les formations et les demandes ponctuelles ;
- l'Excel du personnel ;
- la reprise SharePoint (ADMIN seul).

Il faut **masquer** : Véhicules, Logiciels, Inventaire QR, Mutation, Départ et États. Exchange et l'archivage SharePoint restent **désactivés**.

Ce masquage n'existe pas encore : c'est l'écart E1 ci-dessous.

## 2. Un salarié déjà présent, après reprise

Jeu fictif : `docs/recette-fictive/*.csv`. Nadia Cherif, matricule 00123, déléguée au siège :
- manager Rachid Ouali ;
- trois wilayas (Alger, Blida, Tipaza) ;
- PC-201 et la ligne 0661 00 00 02 détenus depuis le 03/05/2022 ;
- formation réalisée le 12/05/2022 avec une intervenante externe sans compte ;
- ancienne case « Carte de visite » cochée ;
- une ancienne décharge signée et une demande DEM-2026-014 encore ouverte dans SharePoint.

| Élément | Après reprise | Où le RH ou l'IT le retrouve | Reste seulement dans SharePoint / à ressaisir |
|---|---|---|---|
| Identité, matricule « 00123 », contrat, site, poste, département | Conservé, exploitable | Fiche → Situation | — |
| Manager | Relié à la fiche de Rachid Ouali (lien cliquable). Rachid n'a pas de compte | Fiche → Manager | — |
| Trois wilayas | Conservées, filtrables | Fiche → Wilayas ; Personnel → filtre « Wilaya couverte » | — |
| Profil de poste | Relié (« Délégué commercial ») | Fiche → Profil | — |
| Adresse email (cPanel) | Compte consigné | Fiche → Comptes et accès | — |
| Microsoft 365 opérationnel | « Non » repris : case décochée | Fiche → Comptes et accès | — |
| Ligne principale | Rattrapée dès que la SIM est reprise | Fiche → Lignes mobiles, « principale » | — |
| PC-201 et SIM détenus | **Une seule garde chacun**, datée du 03/05/2022, marquée « Reprise SharePoint ». Observation reprise : « Avec sacoche — attribué par Karim IT (ancien) — réf. SharePoint AFF-0045, dossier SharePoint ONB-2022-031 » | Fiche → Biens détenus ; IT : Parc → actif PC-201 → Mouvements | — |
| Ancienne décharge signée | **Aucune décharge** n'est fabriquée pour une remise ancienne. La fiche indique « sans décharge dans l'application » | — | **Le scan signé reste dans la bibliothèque SharePoint** |
| Formation réalisée | Session « réalisée le 12/05/2022 », évaluation « acquis », « animée par Mme Bensalem (intervenante externe) » (sans compte), « enregistrée par » l'ADMIN qui a fait la reprise | Fiche → Formations ; Formations → Soldées | — |
| « Carte de visite » cochée | Texte dans la remarque de reprise (RH, ADMIN) : « n'atteste ni impression ni remise ». Ni tâche, ni preuve | Fiche → Remarque de reprise | — |
| « Poste fixe » | Valeur gardée telle quelle, « sens à confirmer » | Fiche → Informations RH | — |
| Demande DEM-2026-014 encore ouverte | **Non reprise** : la liste « Dossiers » est refusée par la reprise | — | **À ressaisir** par le RH (Nouvelle demande) si elle doit continuer (décision 1) |
| Ancien dossier d'arrivée ONB-2022-031 | Non recréé ; sa référence est citée en texte sur la garde et la formation | Observation de la garde, de la session | Le dossier lui-même et ses tâches |

**Ce que la reprise n'a pas produit** (compteurs relevés en base après la recette) :
- aucune seconde affectation : 2 gardes pour 2 biens ;
- aucune décharge sur un mouvement de reprise ;
- aucun dossier, aucune tâche ;
- aucun avis : tous les avis de l'instance sont « journalisés », aucun envoi.

**Mutation qui change ses wilayas et lui laisse son PC** (jouée jusqu'à la comparaison) :
- Le RH prépare la mutation : site cible Oran, date d'effet 01/12/2026.
- La comparaison propose « Conserver » pour le PC, la SIM et l'adresse email. Elle propose « Ajouter » pour l'écran prévu par le profil, et « Conserver » pour la formation déjà réalisée.
- **Les wilayas ne font pas partie de la mutation** : aucun champ, aucune comparaison, aucune application à la date d'effet.
- Aujourd'hui, le RH ne peut que noter les nouvelles wilayas dans la note RH, puis les modifier à la main sur la fiche le jour venu. Cette modification est immédiate, pas datée, et l'historique des wilayas n'est pas conservé.
- Les wilayas cibles restent **à livrer**.

## 3. Écarts

### Bloquent le pilote

| # | Écart | Exemple concret |
|---|---|---|
| E1 | **Aucun moyen de masquer les modules non recettés.** | Un RH du site pilote voit le bouton « Départ » sur une fiche et lance un départ. La coupure des accès par l'IT n'a jamais été jouée de bout en bout. De même, une « Mutation » ne peut pas changer les wilayas. |
| E2 | **Les dossiers ouverts dans SharePoint au moment de la bascule n'existent pas dans l'application** (décision 1). | Le Graphiste ne voit pas la demande de carte DEM-2026-014 de Nadia tant que le RH ne l'a pas ressaisie. Une arrivée en cours dans SharePoint n'a ni tâche IT ni session de formation dans l'application. |

### Corrigés pendant cette passe (rejoués dans le navigateur)

- **Ligne principale perdue à la reprise** : Personnel est repris avant LignesMobiles. Désormais, elle est rattrapée dès que la SIM est détenue.
- **Affectations reprises après MaterielIT ou LignesMobiles réputées « inchangées »** : la date d'attribution réelle, l'observation, l'intervenant et l'ancienne référence étaient ignorés. Elles complètent maintenant la garde déjà reprise, sans créer de seconde affectation, et jamais si la garde vient d'une remise avec décharge.
- **Observation d'une garde reprise invisible** : elle s'affiche maintenant sur la fiche (biens détenus) et sur l'actif (mouvements).
- **Formation : l'intervenant était forcément un compte**, et c'était par défaut celui qui enregistrait. Désormais, « animée par » (compte ou simple nom, sans compte créé) est distinct de « enregistrée par ». La reprise conserve FormateurNom.
- **Écran de reprise** : il présentait la recréation des parcours en cours comme décidée. Le texte est corrigé.
- **Plantage de la confirmation d'une arrivée** (corrigé le 15/11).

### Peuvent attendre

| # | Écart | Exemple concret |
|---|---|---|
| E3 | Wilayas cibles d'une mutation non livrées. Le changement de wilayas est manuel, immédiat et sans historique. | Le RH qui mute un délégué d'Alger à Oran au 1er décembre doit penser à modifier ses wilayas ce jour-là. Avant cette date, le filtre « Oran » ne le trouve pas, et après, plus rien ne montre qu'il couvrait Blida. |
| E4 | Une adresse email reprise affiche « vérifié le » + la date de la reprise. | L'IT lit « vérifié 16/11/2026 » pour une adresse que personne n'a vérifiée ce jour-là. |
| E5 | Les anciennes décharges signées ne sont pas accessibles depuis l'application. | Avant de restituer le PC-201, l'IT doit chercher le scan de 2022 dans SharePoint. |
| E6 | Après le dépôt d'un scan sur l'écran de remise, l'état reste « signature à recueillir » jusqu'au rechargement. | L'IT croit que le dépôt a échoué et redépose le scan ; une seconde version est alors enregistrée. Rien n'est écrasé. |
| E7 | Modèles de tâches non paramétrables : délai, instructions, ordre d'affichage. | La Formation ne peut pas joindre de consignes standard à la tâche « Formation : Sécurité ». L'échéance est toujours la date d'arrivée. |
| E8 | Une ligne UtilisateursApp « Manager » seule apparaît comme « erreur (rejetée) ». La règle est appliquée et le message la cite désormais, mais la ligne reste comptée parmi les rejets. | L'ADMIN qui reprend les utilisateurs voit des « rejets » qui sont en fait voulus. |
| E9 | Non joué dans le navigateur : États des lieux, Véhicules (hors remise), Logiciels (hors liste), Départ complet, application d'une mutation. | — (modules à masquer pour le pilote) |
| E10 | Non validé sur site : impression A4, étiquettes, douchette, classeur ouvert dans Excel et LibreOffice, rendu des noms arabes dans les PDF. | — |

## 4. Décisions qui demandent réellement votre accord

1. **Anciens dossiers et dossiers en cours (hypothèses H1 et H2).**
   - Proposition : ne pas recréer l'historique des dossiers ; SharePoint reste consultable en lecture seule, et les références sont gardées en texte.
   - Au gel, le RH ressaisit uniquement les dossiers **encore ouverts**, à partir d'une liste extraite de l'export Dossiers.
   - Le cahier T08 demande de « préserver identifiants de liens et références » : c'est pourquoi rien n'est considéré comme validé.
   - L'alternative : recréer chaque dossier comme dossier « historique » en lecture seule. C'est un développement à prévoir.
2. **Masquer les modules non recettés pour le pilote (E1).**
   - Proposition : un réglage de configuration qui masque les menus et refuse les routes de Véhicules, Logiciels, Inventaire QR, Mutation, Départ et États.
   - À livrer avant le pilote si vous êtes d'accord.
3. **Anciennes décharges signées (hypothèse H3).**
   - Proposition : les laisser dans SharePoint et ne rien importer.
   - L'alternative : joindre les scans aux gardes reprises. C'est un développement, et il faut un inventaire préalable de la bibliothèque.

Il reste des points qui ne demandent pas de décision immédiate :
- le sens de « Poste fixe » : la valeur est conservée telle quelle en attendant ;
- les hypothèses mineures « Besoin email pro » (conservé en texte) et « VersionProcessus » (non repris) ;
- l'ordre d'affichage des tâches.
