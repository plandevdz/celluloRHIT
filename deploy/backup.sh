#!/usr/bin/env bash
# Sauvegarde Celluloplast | Parcours salarié.
# Usage (depuis /opt/celluloplast) : sudo deploy/backup.sh [/chemin/destination] [--sans-secrets]
#
# Produit, horodatés :
#   cellulo-<date>.dump          base PostgreSQL (format custom, pg_restore)
#   documents-<date>.tar.gz      décharges PDF, scans signés, spool
#   secrets-<date>.tar.gz        .env + autorité de certification interne de Caddy
#                                (clé privée !) — sauf --sans-secrets
#   sha256-<date>.txt            empreintes des fichiers ci-dessus
#
# Le chiffrement et la copie hors machine restent à organiser (deploy/README.md).
# Le fichier secrets-* ne doit JAMAIS être versionné ni copié en clair.
set -euo pipefail

cd "$(dirname "$0")/.."
DEST="$(realpath -m "${1:-./backups}")"
WITH_SECRETS=1
[ "${2:-}" = "--sans-secrets" ] && WITH_SECRETS=0
PROJECT="${COMPOSE_PROJECT_NAME:-celluloplast}"
STAMP="$(date +%Y%m%d-%H%M%S)"
umask 077
mkdir -p "$DEST"
chmod 700 "$DEST"

echo "Base de données…"
docker compose -p "$PROJECT" exec -T db pg_dump -U cellulo -d cellulo --format=custom > "$DEST/cellulo-$STAMP.dump"

echo "Documents (décharges, scans)…"
docker run --rm \
  -v "${PROJECT}_documents:/data:ro" \
  -v "$DEST":/backup \
  debian:trixie-slim tar czf "/backup/documents-$STAMP.tar.gz" -C /data .

FILES=("cellulo-$STAMP.dump" "documents-$STAMP.tar.gz")
if [ "$WITH_SECRETS" = 1 ]; then
  echo "Configuration et autorité de certification interne (sensible)…"
  # Sans cette CA, une restauration crée une nouvelle autorité : tous les PC
  # Windows devraient réinstaller le certificat racine.
  docker run --rm \
    -v "${PROJECT}_caddy_data:/caddy:ro" \
    -v "$PWD/.env:/cfg/.env:ro" \
    -v "$DEST":/backup \
    debian:trixie-slim tar czf "/backup/secrets-$STAMP.tar.gz" -C / cfg/.env caddy/caddy/pki
  FILES+=("secrets-$STAMP.tar.gz")
fi

( cd "$DEST" && sha256sum "${FILES[@]}" > "sha256-$STAMP.txt" )
chmod 600 "$DEST"/*-"$STAMP".*
echo "Sauvegarde terminée dans $DEST :"
ls -lh "$DEST"/*"$STAMP"*
