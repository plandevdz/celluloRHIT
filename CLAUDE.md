# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Projet

« Celluloplast | Parcours salarié » : application **web** (navigateur, aucun installateur) qui remplace Power Apps / Power Automate pour les arrivées, le parc IT, les formations, les mutations et les départs. Serveur Rust + PostgreSQL + interface React, déployés en **Docker** sur un serveur Debian de l'entreprise et joints par leur IP en HTTPS. Tout (code métier, UI, messages d'erreur, commits) est **en français** ; les noms arabes et les accents doivent être préservés partout.

Le cahier des charges fait foi et se lit dans cet ordre : `01-Metier.md` → `03-UI-UX.md` → `04-Donnees.md` → `02-Technique.md` → `05-Recette.md` (scénarios R01–R14). Le `README.md` porte la préparation du serveur et l'ordre des livraisons. `deploy/README.md` est le guide d'installation et la liste de ce qui est livré ou non. `modeles/` contient les gabarits papier des décharges, que reproduit `crates/server/src/pdf.rs`.

## Commandes

```bash
# Base de développement (PostgreSQL ≥ 15 requis : NULLS NOT DISTINCT, btree_gist)
docker run -d --name cellulo-pg-dev -e POSTGRES_USER=cellulo -e POSTGRES_PASSWORD=devpass -e POSTGRES_DB=cellulo -p 127.0.0.1:55432:5432 postgres:17
export DATABASE_URL=postgres://cellulo:devpass@127.0.0.1:55432/cellulo

cargo test --workspace                       # domaine + tests d'intégration (DATABASE_URL obligatoire)
cargo test -p domain                         # règles pures, sans base
cargo test -p server --test parcours r03     # un seul scénario de recette
cargo run -p server -- serve                 # API + web sur :8080 (migrations au démarrage)
cargo run -p server -- worker                # avis + PDF en attente (processus séparé)
DEMO_PASSWORD=... cargo run -p server -- seed-demo     # données FICTIVES, base vide uniquement
cargo run -p server -- create-admin --email x@y --name "Nom"   # affiche un lien d'activation

cd web && npm run dev        # Vite :5173, relaie /api vers :8080
cd web && npm run build      # tsc --noEmit + build dans web/dist (servi par l'API via WEB_DIR)

docker compose up -d --build # déploiement complet (nécessite .env, voir .env.example)
```

En local, `serve` a besoin de `WEB_DIR=web/dist`, `COOKIE_SECURE=false` et `PUBLIC_URL=http://localhost:8080` ; sous Windows, les polices PDF sont lues dans `C:/Windows/Fonts/arial*.ttf`. Toutes les variables sont dans `crates/server/src/config.rs`.

## Architecture

- `crates/domain` : invariants purs testés sans base. Rôles (exactement 5, cumulables), types de dossier et format des références, routage besoin → équipe et génération des tâches (`needs.rs`), transitions de tâche (`tasks.rs`), règles de remise et de retour partiel (`movements.rs`), normalisation du matricule et clé d'homonymie (`text.rs`). Les énumérations sont stockées en texte via la macro `text_enum!`, avec `as_str`/`parse`.
- `crates/server` : un seul binaire `celluloplast` avec les sous-commandes `serve`, `worker`, `migrate`, `create-admin` et `seed-demo`. Les routes sont dans `src/routes/*.rs`, sous `/api/v1`. Le SQL est écrit directement dans les handlers (sqlx, requêtes à l'exécution, pas de macros `query!`). Toute route inconnue hors `/api` renvoie `web/dist/index.html` (routage côté navigateur).
- `migrations/` : SQL versionné, embarqué par `sqlx::migrate!` (`server::MIGRATOR`). Les contraintes d'unicité et de concurrence vivent **en base** : index partiels, `EXCLUDE` contre le chevauchement des postes, `UNIQUE(case_id, gen_key)` sur les tâches, un seul détenteur actif par actif ou SIM, triggers d'immuabilité (référence de dossier, journal d'audit).
- `web/` : React 18 + react-router 6 + TypeScript, sans librairie d'UI. `api.ts` est le seul point d'accès à l'API. `labels.ts` contient tous les libellés français et les formats de date (fuseau Africa/Algiers). Une page par écran dans `src/pages/`, une feuille de style unique `styles.css`.
- `deploy/` : `Caddyfile` (TLS par CA interne pour une IP), `backup.sh`, guide d'installation. Le `docker-compose.yml` est à la racine et le `Dockerfile` est multi-étapes.

### Motifs à respecter dans toute nouvelle commande

1. **Une transaction par commande**, qui écrit dans la même transaction la mutation métier, `audit::record` et, si une équipe doit être prévenue, `audit::enqueue` (outbox). Jamais d'appel réseau, de PDF ni d'impression dans la transaction : le PDF est lancé après commit (`spawn_pdf`) et le worker rattrape les manqués.
2. **Idempotence** : les créations sensibles lisent `Idempotency-Key` (`idempotency::begin`/`finish`, dans la transaction). La même clé avec le même corps renvoie la même réponse ; une autre clé est refusée avec 422. `POST /arrivals` l'exige. Côté web, la clé est générée une fois par formulaire (`newKey()`) et réutilisée en cas de nouvel essai.
3. **Concurrence** : prise atomique par `UPDATE … WHERE status='a_faire' RETURNING`, sinon 409 avec le détenteur réel. `SELECT … FOR UPDATE` sur les ressources remises. Contrôle de `version` (412 si périmée) sur les saisies concurrentes.
4. **Autorisation côté API à chaque handler** : `user.require(&[Role::…])`. Pour la lecture d'un dossier ou d'un salarié : `can_view_case` / `can_view_employee` (GRAPHISTE et FORMATION ne voient que leurs dossiers ; les notes RH sont masquées hors RH).
5. **Erreurs** : `AppError` → JSON `{error, message, field?, detail?}`. Les violations SQL 23505, 23P01 et 23514 deviennent des 409. Les messages sont rédigés pour l'utilisateur, en français.
6. Les commandes non-GET doivent porter l'en-tête `x-cellulo-client` (protection CSRF, en plus du cookie `SameSite=Strict`). `api.ts` l'ajoute ; les tests aussi (`tests/common/mod.rs`).

### Avis et documents

L'outbox cible `target_roles[]` et/ou `target_user_ids[]`. Le worker résout les destinataires actifs et déduplique par adresse : un utilisateur IT + FORMATION reçoit un seul avis. En V1, seul `MAIL_MODE=journal` existe : rien n'est envoyé, tout est consigné dans `deliveries`. L'adaptateur Microsoft Graph reste à écrire. Les décharges sont stockées sous `DATA_DIR/dossiers/<REF>/<remises|restitutions>/<MVT>/v1-original.pdf`. Les scans sont contrôlés par signature binaire et versionnés, jamais écrasés. Le statut `signe` exige un scan.

## Invariants métier (rappel, détails dans les cahiers)

- Une réponse « vide » vaut « à préciser », jamais « non » ; seul « oui » génère du travail. La préférence de PC du RH n'est jamais réécrite : une substitution par l'IT exige un motif.
- Préparé ≠ remis, imprimé ≠ signé. Une tâche PC, téléphone ou SIM ne se termine qu'après une remise liée (ou avec une observation). Une tâche email, Xena ou Intelix ne se termine qu'après la consignation du compte réel.
- Une restitution référence la ligne de remise d'origine et ne solde que la quantité rendue.
- Une demande ponctuelle (DEM) ne rouvre jamais son dossier parent. Un import ou une fiche créée hors parcours n'annonce rien.
- Les dates RH (`date`) sont distinctes des instants (`timestamptz` en UTC, affichés en Africa/Algiers, UTC+1 fixe).
- Aucun mot de passe de salarié et aucun secret dans le dépôt, l'image ou les logs. Les tests et démonstrations n'utilisent que des données fictives (`@exemple.test`).

## Git

Dépôt : https://github.com/plandevdz/celluloRHIT. L'utilisateur travaille directement sur `main` (pas de branches).
