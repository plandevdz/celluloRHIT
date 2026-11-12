#!/usr/bin/env bash
# Sauvegarde Celluloplast | Parcours salarié.
# Usage (depuis /opt/celluloplast) : sudo deploy/backup.sh [/chemin/destination] [--sans-secrets]
#
# Chiffrement à clé publique (GnuPG), dès l'écriture : les flux passent de
# tar/pg_dump à gpg sans jamais être écrits en clair. Le serveur ne détient
# que la clé PUBLIQUE de récupération ; la clé privée reste hors du serveur.
#
# Configuration (/etc/celluloplast/backup.conf, ou variables d'environnement) :
#   BACKUP_GPG_RECIPIENT=<empreinte de la clé de récupération>
#   BACKUP_GPG_PUBKEY=/etc/celluloplast/backup-pubkey.asc
#
# Fichiers produits (horodatés) :
#   cellulo-<date>.dump[.gpg]          base PostgreSQL (pg_dump, format custom)
#   documents-<date>.tar.gz[.gpg]      décharges PDF et scans
#   secrets-<date>.tar.gz.gpg          .env + CA interne Caddy (clé privée) — TOUJOURS chiffré
#   sha256-<date>.txt                  empreintes des fichiers ci-dessus
#
# Codes de sortie : 0 sauvegarde complète chiffrée (ou partielle demandée par
# --sans-secrets) ; 3 sauvegarde INCOMPLÈTE (chiffrement non configuré :
# secrets non sauvegardés) ; autre : échec.
set -euo pipefail

cd "$(dirname "$0")/.."
CONF="${BACKUP_CONF:-/etc/celluloplast/backup.conf}"
# shellcheck disable=SC1090
[ -f "$CONF" ] && . "$CONF"

DEST="$(realpath -m "${1:-./backups}")"
WITH_SECRETS=1
[ "${2:-}" = "--sans-secrets" ] && WITH_SECRETS=0
PROJECT="${COMPOSE_PROJECT_NAME:-celluloplast}"
RECIPIENT="${BACKUP_GPG_RECIPIENT:-}"
PUBKEY="${BACKUP_GPG_PUBKEY:-/etc/celluloplast/backup-pubkey.asc}"
STAMP="$(date +%Y%m%d-%H%M%S)"
umask 077
mkdir -p "$DEST"
chmod 700 "$DEST"

WORK_GNUPG=""
cleanup() {
  local rc=$?   # le nettoyage ne doit jamais masquer le code de sortie (3 = incomplète)
  rm -f "$DEST"/*.partiel 2>/dev/null || true
  if [ -n "$WORK_GNUPG" ]; then rm -rf "$WORK_GNUPG"; fi
  exit "$rc"
}
trap cleanup EXIT

# ---------------------------------------------------------------- chiffrement
CRYPTO=0
if [ -n "$RECIPIENT" ]; then
  if [ ! -f "$PUBKEY" ]; then
    echo "ERREUR : clé publique de récupération introuvable ($PUBKEY)." >&2
    exit 1
  fi
  # Trousseau temporaire ne contenant QUE la clé publique configurée.
  WORK_GNUPG="$(mktemp -d)"
  export GNUPGHOME="$WORK_GNUPG"
  gpg --batch --quiet --import "$PUBKEY" 2>/dev/null
  WANT="$(echo "$RECIPIENT" | tr -d ' ' | tr '[:lower:]' '[:upper:]')"
  if ! gpg --batch --with-colons --fingerprint 2>/dev/null | awk -F: '$1=="fpr"{print $10}' | grep -qx "$WANT"; then
    echo "ERREUR : l'empreinte BACKUP_GPG_RECIPIENT ne correspond pas à la clé de $PUBKEY." >&2
    exit 1
  fi
  CRYPTO=1
fi

# Lit un flux sur l'entrée standard et l'écrit chiffré (ou en clair si
# autorisé) dans un fichier temporaire « .partiel ».
sink() {
  local out="$1" mode="$2"
  if [ "$mode" = chiffre ]; then
    gpg --batch --yes --quiet --trust-model always --encrypt --recipient "$WANT" --output "$out"
  else
    cat > "$out"
  fi
}

# run_to <fichier final> <chiffre|clair> <commande productrice…>
# Le fichier final n'apparaît que si toute la chaîne a réussi.
FILES=()
run_to() {
  local out="$1" mode="$2"
  shift 2
  if "$@" | sink "$out.partiel" "$mode"; then
    mv "$out.partiel" "$out"
    FILES+=("$(basename "$out")")
  else
    rm -f "$out.partiel"
    echo "ERREUR : échec de l'écriture de $(basename "$out")" >&2
    exit 1
  fi
}

if [ "$CRYPTO" = 1 ]; then MODE=chiffre; EXT=".gpg"; else MODE=clair; EXT=""; fi

echo "Base de données…"
run_to "$DEST/cellulo-$STAMP.dump$EXT" "$MODE" \
  docker compose -p "$PROJECT" exec -T db pg_dump -U cellulo -d cellulo --format=custom

echo "Documents (décharges, scans)…"
run_to "$DEST/documents-$STAMP.tar.gz$EXT" "$MODE" \
  docker run --rm -v "${PROJECT}_documents:/data:ro" debian:trixie-slim tar czf - -C /data .

STATUS="complete"
if [ "$WITH_SECRETS" = 0 ]; then
  STATUS="partielle-demandee"
elif [ "$CRYPTO" = 1 ]; then
  echo "Configuration et autorité de certification interne (chiffrées)…"
  # Sans cette CA, une restauration crée une nouvelle autorité : tous les PC
  # Windows devraient réinstaller le certificat racine.
  run_to "$DEST/secrets-$STAMP.tar.gz.gpg" chiffre \
    docker run --rm \
      -v "${PROJECT}_caddy_data:/caddy:ro" \
      -v "$PWD/.env:/cfg/.env:ro" \
      debian:trixie-slim tar czf - -C / cfg/.env caddy/caddy/pki
else
  STATUS="incomplete"
fi

( cd "$DEST" && sha256sum "${FILES[@]}" > "sha256-$STAMP.txt" )
chmod 600 "$DEST"/*"$STAMP"*
echo "Fichiers écrits dans $DEST :"
ls -l "$DEST"/*"$STAMP"*

case "$STATUS" in
  complete)
    echo "Sauvegarde COMPLÈTE, chiffrée pour la clé $WANT."
    exit 0 ;;
  partielle-demandee)
    echo "Sauvegarde PARTIELLE (--sans-secrets) : .env et CA Caddy non sauvegardés."
    exit 0 ;;
  incomplete)
    echo "SAUVEGARDE INCOMPLÈTE : chiffrement non configuré (BACKUP_GPG_RECIPIENT)." >&2
    echo "  .env et CA Caddy NON sauvegardés ; base et documents écrits EN CLAIR." >&2
    echo "  Configurer la clé de récupération : voir deploy/README.md, « Sauvegarde et restauration »." >&2
    exit 3 ;;
esac
