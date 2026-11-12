#!/usr/bin/env bash
# Restauration ISOLÉE de recette — UNIQUEMENT sur une VM ou une machine de
# test distincte de la production. Projet Docker fixe : cellulo-restore.
#
# Préalable (voir deploy/README.md) : sur le poste d'administration, les
# sauvegardes chiffrées ont été vérifiées (sha256sum -c) puis déchiffrées
# vers le compte nominatif de la machine de test, sans sudo :
#   ~/restau/cellulo.dump  ~/restau/documents.tar.gz  ~/restau/secrets.tar.gz
#
# Usage (sur la machine de test, dans /opt/celluloplast, session interactive) :
#   sudo deploy/restore-isole.sh --source ~/restau --adresse <IP-de-la-VM> \
#        --machine-de-test-confirmee [--empreinte-ca-attendue <SHA256>] [--supprimer-source]
#
# Toute étape en échec arrête la procédure : l'application n'est démarrée
# qu'après la restauration réussie de la base, des documents et de la CA,
# et aucun fichier n'est nettoyé tant que tous les contrôles ne sont pas OK.
set -euo pipefail

PROJECT=cellulo-restore          # jamais le projet de production « celluloplast »
PROD_PROJECT=celluloplast
CA_REL=caddy/caddy/pki/authorities/local/root.crt
IMG=debian:trixie-slim

die()   { echo "ARRÊT : $*" >&2; exit 1; }
etape() { echo; echo "== $*"; }

# ---------------------------------------------------------------- arguments
SOURCE="" ADDR="" CONFIRM=0 EXPECTED_CA="" PURGE_SOURCE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --source) SOURCE="${2:-}"; shift 2 ;;
    --adresse) ADDR="${2:-}"; shift 2 ;;
    --machine-de-test-confirmee) CONFIRM=1; shift ;;
    --empreinte-ca-attendue) EXPECTED_CA="$(echo "${2:-}" | tr -d ': ' | tr '[:lower:]' '[:upper:]')"; shift 2 ;;
    --supprimer-source) PURGE_SOURCE=1; shift ;;
    *) die "option inconnue : $1" ;;
  esac
done
[ "$CONFIRM" = 1 ] || die "réservé à une VM ou machine de test distincte de la production : ajoutez --machine-de-test-confirmee"
[ -n "$SOURCE" ] && [ -n "$ADDR" ] || die "--source et --adresse sont obligatoires"
[ "$(id -u)" = 0 ] || die "lancez le script avec sudo (session interactive : sudo peut demander votre mot de passe)"
for c in docker openssl sha256sum tar; do command -v "$c" >/dev/null || die "commande manquante : $c"; done
cd "$(dirname "$0")/.."
[ -f docker-compose.yml ] || die "docker-compose.yml introuvable (lancer depuis le dépôt)"
for f in cellulo.dump documents.tar.gz secrets.tar.gz; do
  [ -r "$SOURCE/$f" ] && [ -s "$SOURCE/$f" ] || die "fichier source manquant ou vide : $SOURCE/$f"
done

# ---------------------------------------------------------------- garde-fous
etape "1. Garde-fous : pas de production sur cette machine, cible vide"
[ -z "$(docker ps -a -q --filter "label=com.docker.compose.project=$PROD_PROJECT")" ] \
  || die "des conteneurs du projet de PRODUCTION « $PROD_PROJECT » existent sur cette machine"
[ -z "$(docker volume ls -q --filter "label=com.docker.compose.project=$PROD_PROJECT")" ] \
  || die "des volumes du projet de PRODUCTION « $PROD_PROJECT » existent sur cette machine"
[ -z "$(docker ps -a -q --filter "label=com.docker.compose.project=$PROJECT")" ] \
  || die "cible non vide : des conteneurs « $PROJECT » existent (docker compose -p $PROJECT down -v)"
[ -z "$(docker volume ls -q --filter "label=com.docker.compose.project=$PROJECT")" ] \
  || die "cible non vide : des volumes « $PROJECT » existent (docker compose -p $PROJECT down -v)"
echo "   OK : aucune trace de production, projet $PROJECT vierge"

WORK="$(mktemp -d)"          # 700, propriété de root
chmod 700 "$WORK"
echo "   espace de travail : $WORK (conservé en cas d'échec)"
mkdir "$WORK/secrets"
tar xzf "$SOURCE/secrets.tar.gz" -C "$WORK/secrets" || die "archive des secrets illisible"
[ -f "$WORK/secrets/$CA_REL" ] || die "certificat racine Caddy absent de la sauvegarde ($CA_REL)"
PROD_ADDR="$(sed -n 's/^SERVER_ADDRESS=//p' "$WORK/secrets/cfg/.env" 2>/dev/null | tr -d '"' | tr -d ' ' || true)"
case ",$PROD_ADDR," in
  *",$ADDR,"*) die "--adresse $ADDR est l'adresse de PRODUCTION de la sauvegarde : utilisez l'adresse de la VM de test" ;;
esac

etape "2. Certificat racine Caddy de la sauvegarde"
CA_BACKUP="$(openssl x509 -noout -fingerprint -sha256 -in "$WORK/secrets/$CA_REL" | cut -d= -f2 | tr -d ':')"
echo "   empreinte SHA-256 (sauvegarde) : $CA_BACKUP"
if [ -n "$EXPECTED_CA" ] && [ "$EXPECTED_CA" != "$CA_BACKUP" ]; then
  die "l'empreinte de la CA sauvegardée ne correspond pas à --empreinte-ca-attendue"
fi

# Configuration propre à la recette : adresse de la VM, mot de passe
# PostgreSQL neuf (celui de la production n'est pas réutilisé), aucun
# destinataire de test ; MAIL_MODE=journal est imposé par docker-compose.yml.
# Conservé hors de l'espace de travail : toutes les commandes manuelles
# ultérieures l'utilisent (docker compose -p cellulo-restore --env-file …).
ENVF="${RESTORE_ENV_FILE:-/root/cellulo-restore.env}"
( umask 077
{
  echo "SERVER_ADDRESS=$ADDR"
  echo "TLS_DEFAULT_SNI=$ADDR"
  echo "PUBLIC_URL=https://$ADDR"
  echo "POSTGRES_PASSWORD=$(openssl rand -hex 24)"
  echo "MAIL_TEST_RECIPIENT="
} > "$ENVF" )
echo "   environnement de recette : $ENVF"
DC=(docker compose -p "$PROJECT" --env-file "$ENVF")

etape "3. Documents et CA dans des volumes neufs du projet $PROJECT"
for v in documents caddy_data; do
  docker volume create --label "com.docker.compose.project=$PROJECT" --label "com.docker.compose.volume=$v" "${PROJECT}_$v" >/dev/null
done
docker run --rm -v "${PROJECT}_documents:/data" -v "$(realpath "$SOURCE"):/src:ro" "$IMG" \
  sh -c 'tar xzf /src/documents.tar.gz -C /data && chown -R 10001 /data' || die "restauration des documents"
docker run --rm -v "${PROJECT}_caddy_data:/caddy" -v "$WORK/secrets:/src:ro" "$IMG" \
  sh -c 'mkdir -p /caddy/caddy && cp -a /src/caddy/caddy/pki /caddy/caddy/' || die "restauration de la CA"

etape "4. Base : démarrage, vérification de la cible, pg_restore"
"${DC[@]}" up -d db
for i in $(seq 1 60); do
  "${DC[@]}" exec -T db pg_isready -U cellulo -d cellulo >/dev/null 2>&1 && break
  [ "$i" = 60 ] && die "PostgreSQL ne répond pas"
  sleep 2
done
TARGET="$("${DC[@]}" exec -T db psql -U cellulo -d cellulo -tAc \
  "select current_database() || '|' || (select count(*) from information_schema.tables where table_schema = 'public')")"
[ "$TARGET" = "cellulo|0" ] || die "base cible non vide ou inattendue ($TARGET) : pg_restore refusé"
echo "   OK : base « cellulo » vide dans le projet $PROJECT"
# Lecture du dump par root (ce script), transmis sur l'entrée standard.
"${DC[@]}" exec -T db pg_restore -U cellulo -d cellulo --no-owner --exit-on-error < "$SOURCE/cellulo.dump" \
  || die "pg_restore en échec : l'application n'est PAS démarrée"
echo "   OK : base restaurée"

etape "5. Démarrage de l'application (projet $PROJECT, mode journal)"
"${DC[@]}" up -d --build

etape "6. Contrôles"
FAILS=0
q() { "${DC[@]}" exec -T db psql -U cellulo -d cellulo -tAc "$1"; }
echo "   dossiers : $(q "select count(*) from cases") ; derniers : $(q "select string_agg(reference, ', ') from (select reference from cases order by created_at desc limit 5) x")"
echo "   rôles :"
q "select '      ' || u.email || ' : ' || string_agg(r.role, ' + ' order by r.role) from users u join user_roles r on r.user_id = u.id group by u.email order by u.email"

DOCS="$(q "select sha256 || '  ' || storage_path from document_versions order by created_at")"
NDOCS="$(printf '%s\n' "$DOCS" | grep -c . || true)"
if [ "$NDOCS" -gt 0 ]; then
  CHECK="$(printf '%s\n' "$DOCS" | docker run -i --rm -v "${PROJECT}_documents:/data:ro" "$IMG" sh -c 'cd /data && sha256sum -c -' 2>&1 || true)"
  NOK="$(printf '%s\n' "$CHECK" | grep -c ': OK$' || true)"
  echo "   empreintes PDF/scans : $NOK/$NDOCS conformes (sha256 en base = fichier restauré)"
  [ "$NOK" = "$NDOCS" ] || { FAILS=$((FAILS + 1)); printf '%s\n' "$CHECK" | grep -v ': OK$' | sed 's/^/      /'; }
else
  echo "   empreintes PDF/scans : aucun document dans la sauvegarde"
fi

sleep 5   # laisser Caddy démarrer : il ne doit PAS régénérer une autorité
CA_AFTER="$(docker run --rm -v "${PROJECT}_caddy_data:/caddy:ro" "$IMG" cat "/caddy/$CA_REL" \
  | openssl x509 -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ':')"
if [ "$CA_AFTER" = "$CA_BACKUP" ]; then
  echo "   certificat racine Caddy : identique avant/après ($CA_AFTER)"
else
  echo "   certificat racine Caddy : DIFFÉRENT (sauvegarde $CA_BACKUP, après $CA_AFTER)"; FAILS=$((FAILS + 1))
fi

MODE="$("${DC[@]}" exec -T app printenv MAIL_MODE | tr -d '\r')"
echo "   mode d'envoi : $MODE ; remises d'avis : $(q "select coalesce(string_agg(status || '=' || n, ', '), 'aucune') from (select status, count(*) n from deliveries group by status) x")"
[ "$MODE" = journal ] || { echo "   MAIL_MODE inattendu"; FAILS=$((FAILS + 1)); }

if [ "$FAILS" -gt 0 ]; then
  echo
  echo "RECETTE EN ÉCHEC : $FAILS contrôle(s). Fichiers conservés pour analyse : $WORK et $SOURCE" >&2
  exit 1
fi

etape "7. Nettoyage des fichiers en clair (après succès uniquement)"
find "$WORK" -type f -exec shred -u {} + 2>/dev/null || true
rm -rf "$WORK"
if [ "$PURGE_SOURCE" = 1 ]; then
  shred -u "$SOURCE/cellulo.dump" "$SOURCE/documents.tar.gz" "$SOURCE/secrets.tar.gz"
fi
echo
echo "RECETTE DE RESTAURATION RÉUSSIE (projet $PROJECT)."
echo "Commandes manuelles : sudo docker compose -p $PROJECT --env-file $ENVF <commande>"
echo "Démontage après recette : sudo docker compose -p $PROJECT --env-file $ENVF down -v"
