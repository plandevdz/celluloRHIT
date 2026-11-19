# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Projet

« Celluloplast | Parcours salarié » : application **web** (navigateur, aucun installateur) qui remplace Power Apps / Power Automate pour les arrivées, le parc IT, les formations, les mutations et les départs. Serveur Rust + PostgreSQL + interface React, déployés en **Docker** sur un serveur Debian de l'entreprise et joints par leur IP en HTTPS. Tout (code métier, UI, messages d'erreur, commits) est **en français** ; les noms arabes et les accents doivent être préservés partout.

Le cahier des charges fait foi et se lit dans cet ordre : `01-Metier.md` → `03-UI-UX.md` → `04-Donnees.md` → `02-Technique.md` → `05-Recette.md` (scénarios R01–R14). Le `README.md` porte la préparation du serveur et l'ordre des livraisons. `deploy/README.md` est le guide d'installation et la liste de ce qui est livré ou non ; `deploy/certificat-intune.md` (avec `deploy/intune/Generer-ScriptsIntune.ps1`) le déploiement du certificat racine HTTPS sur les PC par Intune. `modeles/` contient les gabarits papier des décharges, que reproduit `crates/server/src/pdf.rs`. `docs/correspondance-sharepoint.md` donne la destination des 184 champs de l'ancienne application (à tenir à jour si un champ change de place) ; `docs/pilote.md` le périmètre du pilote, les écarts et les décisions en attente (l'ancienne application Power Apps n'a jamais été en production : ses listes ne sont que des données de départ) ; `docs/recette-pilote.md` la fiche de recette.

## Commandes

```bash
# Base de développement (PostgreSQL ≥ 15 requis : NULLS NOT DISTINCT, btree_gist)
docker run -d --name cellulo-pg-dev -e POSTGRES_USER=cellulo -e POSTGRES_PASSWORD=devpass -e POSTGRES_DB=cellulo -p 127.0.0.1:55432:5432 postgres:17
export DATABASE_URL=postgres://cellulo:devpass@127.0.0.1:55432/cellulo

cargo test --workspace                       # domaine + tests d'intégration (DATABASE_URL obligatoire)
cargo test -p domain                         # règles pures, sans base
cargo test -p server --test parcours r03     # un seul scénario de recette
cargo test -p server --test lot1             # périmètre, présence, preuves, avis
cargo test -p server --test lot2_excel       # import/export Excel
cargo test -p server --test livraison1       # 9 scénarios de la 1re livraison (fiche, profils, équipements, Excel v2)
cargo test -p server --test arbitrages       # règles provisoires du 16/11 (intervenant sans compte, cases anciennes, salarié déjà présent)
cargo test -p server --test pilote           # mode pilote : modules fermés, besoin véhicule, anciens avis, preuves anciennes
cargo test -p server --test administration   # réglages de l'interface, certificat, modèles de tâches, listes par équipe, sauvegarde (GPG_BIN, PG_DUMP_BIN ≥ 17 ; ignoré sans ces outils)
cargo test -p server --test mutation_depart  # R09, R10 (aussi : vehicules, logiciels, inventaire, graph, reprise)
cargo run -p server -- serve                 # API + web sur :8080 (migrations au démarrage)
cargo run -p server -- worker                # avis + PDF en attente (processus séparé)
DEMO_PASSWORD=... cargo run -p server -- seed-demo     # données FICTIVES, base vide uniquement
cargo run -p server -- create-admin --email x@y --name "Nom"   # affiche un lien d'activation

cd web && npm run dev        # Vite :5173, relaie /api vers :8080
cd web && npm run build      # tsc --noEmit + build dans web/dist (servi par l'API via WEB_DIR)

docker compose up -d --build # déploiement complet (nécessite .env, voir .env.example)
```

`MODULES_DESACTIVES` (`pilote` ou liste : vehicules, logiciels, inventaire, mutation, depart, etats) ferme des modules : menus masqués (`useAuth().on`), routes refusées (`routes::module_of`), traitements du worker arrêtés ; toute nouvelle route d'un module facultatif doit être ajoutée à `module_of`. En local, `serve` a besoin de `WEB_DIR=web/dist`, `COOKIE_SECURE=false` et `PUBLIC_URL=http://localhost:8080` ; sous Windows, les polices PDF sont lues dans `C:/Windows/Fonts/arial*.ttf`. Toutes les variables sont dans `crates/server/src/config.rs`. Depuis la migration 0009, l'ADMIN peut remplacer dans l'interface les sections modules, mail, archive, échéances et sauvegarde (`runtime_settings`, `settings.rs`) : le code lit toujours la configuration **effective** par `state.cfg()` (jamais un champ figé au démarrage), et les clients Graph par `state.mailer()` / `state.archiver()`. Sauvegarde intégrée : `backup.rs` (worker, gpg + pg_dump, `BACKUP_DIR`, `GPG_BIN`, `PG_DUMP_BIN`, `CADDY_DATA_DIR`) ; certificats Graph générés sur le serveur : `certs.rs` (`DATA_DIR/secrets`).

## Architecture

- `crates/domain` : invariants purs testés sans base. Rôles (exactement 5, cumulables), types de dossier et format des références, routage besoin → équipe et génération des tâches (`needs.rs`), transitions de tâche (`tasks.rs`), règles de remise et de retour partiel (`movements.rs`), normalisation du matricule et clé d'homonymie (`text.rs`), véhicules et kilométrage (`vehicles.rs`), classement d'un pointage d'inventaire (`inventory.rs`), écarts de mutation et état des accès au départ (`transitions.rs`), correspondance des choix SharePoint (`legacy.rs`). Les énumérations sont stockées en texte via la macro `text_enum!`, avec `as_str`/`parse`.
- `crates/server` : un seul binaire `celluloplast` avec les sous-commandes `serve`, `worker`, `migrate`, `create-admin` et `seed-demo`. Modules du lot 3 : `routes/vehicles.rs`, `software.rs`, `inventory.rs` (+ `labels.rs`, étiquettes QR), `transfers.rs` (M05), `departures.rs` (M06), `referentials.rs` (wilayas, profils), `reprise.rs` (+ `tabular.rs`, lecture CSV/xlsx), `reports.rs` (états) ; `graph.rs` (client Microsoft Graph). Le worker (`worker.rs`) traite aussi les échéances, l'application des mutations, les coupures et sorties, l'envoi Graph et l'archivage ; tout repose sur l'état en base (rejouable après redémarrage). Les routes sont dans `src/routes/*.rs`, sous `/api/v1`. Le SQL est écrit directement dans les handlers (sqlx, requêtes à l'exécution, pas de macros `query!`). Toute route inconnue hors `/api` renvoie `web/dist/index.html` (routage côté navigateur).
- `migrations/` : SQL versionné, embarqué par `sqlx::migrate!` (`server::MIGRATOR`). Les contraintes d'unicité et de concurrence vivent **en base** : index partiels, `EXCLUDE` contre le chevauchement des postes, `UNIQUE(case_id, gen_key)` sur les tâches, un seul détenteur actif par actif ou SIM, triggers d'immuabilité (référence de dossier, journal d'audit).
- `web/` : React 18 + react-router 6 + TypeScript, sans librairie d'UI. `api.ts` est le seul point d'accès à l'API. `labels.ts` contient tous les libellés français et les formats de date (fuseau Africa/Algiers). Une page par écran dans `src/pages/`, une feuille de style unique `styles.css`.
- `deploy/` : `Caddyfile` (TLS par CA interne pour une IP), `backup.sh`, guide d'installation. Le `docker-compose.yml` est à la racine et le `Dockerfile` est multi-étapes.

### Motifs à respecter dans toute nouvelle commande

1. **Une transaction par commande**, qui écrit dans la même transaction la mutation métier, `audit::record` et, si une équipe doit être prévenue, `audit::enqueue` (outbox). Jamais d'appel réseau, de PDF ni d'impression dans la transaction : le PDF est lancé après commit (`spawn_pdf`) et le worker rattrape les manqués.
2. **Idempotence** : les créations sensibles lisent `Idempotency-Key` (`idempotency::begin`/`finish`, dans la transaction). La même clé avec le même corps renvoie la même réponse ; une autre clé est refusée avec 422. `POST /arrivals` l'exige. Côté web, la clé est générée une fois par formulaire (`newKey()`) et réutilisée en cas de nouvel essai.
3. **Concurrence** : prise atomique par `UPDATE … WHERE status='a_faire' RETURNING`, sinon 409 avec le détenteur réel. `SELECT … FOR UPDATE` sur les ressources remises. Contrôle de `version` (412 si périmée) sur les saisies concurrentes.
4. **Autorisation côté API à chaque handler** : `user.require(&[Role::…])`. Pour la lecture d'un dossier ou d'un salarié : `can_view_case` / `can_view_employee`, puis **filtrage des données renvoyées** (pas seulement des boutons) : hors RH/ADMIN, `GET /cases/{ref}` ne renvoie que les tâches et besoins des rôles de l'utilisateur, sans note RH, autres dossiers, suivi des avis ni historique étranger ; les décharges sont réservées à RH/ADMIN/IT ; `GET /employees/{id}` en vue « equipe » (GRAPHISTE/FORMATION) n'a ni historique des postes, ni biens, ni matricule. Tout nouveau champ doit être classé dans ce périmètre et couvert par un test d'**accès autorisé** (voir `tests/lot1.rs`).
5. **Erreurs** : `AppError` → JSON `{error, message, field?, detail?}`. Les violations SQL 23505, 23P01 et 23514 deviennent des 409. Les messages sont rédigés pour l'utilisateur, en français.
6. Les commandes non-GET doivent porter l'en-tête `x-cellulo-client` (protection CSRF, en plus du cookie `SameSite=Strict`). `api.ts` l'ajoute ; les tests aussi (`tests/common/mod.rs`).

### Fin de tâche, présence, import

- `POST /tasks/{id}/complete` exige une résolution : `remis` (lignes de remise du dossier de la bonne catégorie et quantité : `NeedCategory::matches_item`), `deja_detenu` (affectations actives vérifiées), `exception` (motif obligatoire, jamais comptée comme remise) ou `realise` (compte consigné pour email/Xena/Intelix). Les formations se soldent via leur session. Les tâches terminées avant la migration 0002 ont `resolution` NULL et ne doivent pas être requalifiées.
- `POST /employees/{id}/arrival` confirme la présence (statut `actif`), indépendamment de la clôture du dossier ; idempotent ; correction motivée.
- Import Excel (`routes/imports.rs`, `personnel_xlsx.rs`) : aperçu sans écriture métier → décisions → `commit` transactionnel (SAVEPOINT par ligne). Ne jamais rapprocher sur le seul nom ; jamais d'arrivée, de tâche ni d'avis à l'import. Un classeur (SHA-256) n'est appliqué qu'une fois.

### Avis et documents

L'outbox cible `target_roles[]` et/ou `target_user_ids[]`. Le worker résout les destinataires actifs, déduplique par adresse (un utilisateur IT + FORMATION reçoit un seul avis) et note dans `missing_roles` les équipes sans destinataire. `MAIL_MODE=journal` (défaut) n'envoie rien ; `MAIL_MODE=graph` envoie par Microsoft Graph (certificat, `./secrets`, jamais dans le dépôt) : 202 = « accepté », jamais « livré » ; délai dépassé après soumission = `a_verifier`, jamais réexpédié sans reprise motivée ; `MAIL_ALLOWED_DOMAINS` limite le pilote. Les avis émis par le serveur (échéances, coupures) passent par `worker::enqueue_system`. L'interface ne doit jamais présenter « consigné » comme « envoyé » : utiliser `visibilityText` (labels.ts) et les compteurs `logged` / `accepted` / `pending` / `failed`. Les tests Graph utilisent un faux serveur local (`tests/graph.rs`) : ne jamais appeler Microsoft depuis un test. Les décharges sont stockées sous `DATA_DIR/dossiers/<REF>/<remises|restitutions>/<MVT>/v1-original.pdf`. Les scans sont contrôlés par signature binaire et versionnés, jamais écrasés. Le statut `signe` exige un scan.

### Mutation, départ, reprise

- Une mutation ne modifie jamais la situation avant sa date d'effet ; le worker l'applique une seule fois (`transfers.applied_position_id` unique) et la suspend si le poste a changé depuis la comparaison. Après application, seule une correction datée (`POST /employees/{id}/positions`) est possible.
- Un départ sépare `last_day` (date RH) et `cutoff_at` (instant UTC). Aucun compte n'est réputé coupé sans confirmation de l'IT sur son contrôle de sortie ; les tâches de contrôle (`tasks.exit_control_id`) ne se terminent pas par `/tasks/{id}/complete`. Le motif privé (`departures.private_note`) n'est rendu qu'au RH et à l'ADMIN. Une restitution solde le contrôle du bien (`departures::on_returned`).
- La reprise SharePoint (`/reprise/...`, ADMIN) suit le même schéma que l'import Excel ; l'ancien ID est conservé dans `legacy_ids` ; jamais de rapprochement automatique par le nom seul ; aucune arrivée, tâche ni avis ; les gardes reprises sont des mouvements `reprise` sans décharge.

## Invariants métier (rappel, détails dans les cahiers)

- Une réponse « vide » vaut « à préciser », jamais « non » ; seul « oui » génère du travail. La préférence de PC du RH n'est jamais réécrite : une substitution par l'IT exige un motif.
- Préparé ≠ remis, imprimé ≠ signé. Une tâche PC, téléphone ou SIM ne se termine qu'après une remise liée (ou avec une observation). Une tâche email, Xena ou Intelix ne se termine qu'après la consignation du compte réel.
- Une restitution référence la ligne de remise d'origine et ne solde que la quantité rendue.
- Une demande ponctuelle (DEM) ne rouvre jamais son dossier parent. Un import ou une fiche créée hors parcours n'annonce rien.
- Les dates RH (`date`) sont distinctes des instants (`timestamptz` en UTC, affichés en Africa/Algiers, UTC+1 fixe).
- Aucun mot de passe de salarié et aucun secret dans le dépôt, l'image ou les logs. Les tests et démonstrations n'utilisent que des données fictives (`@exemple.test`).

## Git

Dépôt : https://github.com/plandevdz/celluloRHIT (potentiellement public : aucun secret, vrai fichier du personnel ni scan réel ; les exports SharePoint locaux `Schema-Listes.*` sont ignorés). Suivre la consigne de chaque demande : les premières tranches ont été poussées sur `main` ; les lots 1 et 2 ont été développés sur la branche `lot1-fiabilisation-lot2-excel` (fusionnée par PR #1) ; le lot 3 (application complète) sur `lot3-complet`. Ne jamais déployer ni se connecter au serveur de production sans demande explicite.

## Environnement de développement (poste Windows)

Docker Desktop peut être indisponible : un PostgreSQL 17 portable (binaires EDB, `initdb`/`pg_ctl` ; si 55432 tombe dans une plage de ports réservée par Windows, utiliser 54329) suffit pour les tests. Le disque C: est presque plein : compiler avec `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0` réduit `target/` à ~1 Go ; un `celluloplast.exe` en cours d'exécution bloque la recompilation (arrêter les serveurs locaux d'abord).
