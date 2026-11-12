#!/usr/bin/env bash
# Sauvegarde Celluloplast | Parcours salarié : base PostgreSQL + documents.
# Usage (depuis /opt/celluloplast) : sudo deploy/backup.sh [/chemin/destination]
# La copie hors machine et le chiffrement restent à organiser (voir deploy/README.md).
set -euo pipefail

cd "$(dirname "$0")/.."
DEST="$(realpath -m "${1:-./backups}")"
STAMP="$(date +%Y%m%d-%H%M%S)"
mkdir -p "$DEST"
chmod 700 "$DEST"

echo "Base de données…"
docker compose exec -T db pg_dump -U cellulo -d cellulo --format=custom > "$DEST/cellulo-$STAMP.dump"

echo "Documents (décharges, scans)…"
docker run --rm \
  -v celluloplast_documents:/data:ro \
  -v "$DEST":/backup \
  debian:trixie-slim tar czf "/backup/documents-$STAMP.tar.gz" -C /data .

( cd "$DEST" && sha256sum "cellulo-$STAMP.dump" "documents-$STAMP.tar.gz" > "sha256-$STAMP.txt" )
chmod 600 "$DEST"/*-"$STAMP".*
echo "Sauvegarde terminée dans $DEST :"
ls -lh "$DEST"/*"$STAMP"*
