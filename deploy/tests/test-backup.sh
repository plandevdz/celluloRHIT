#!/usr/bin/env bash
# Test de deploy/backup.sh avec des secrets FICTIFS et un faux « docker ».
# Vérifie : chiffrement dès l'écriture, aucun secret en clair sur disque,
# déchiffrement avec la clé privée tenue HORS du serveur, refus et code 3
# si le chiffrement n'est pas configuré, empreinte refusée si incohérente.
# Usage : bash deploy/tests/test-backup.sh   (nécessite gpg, tar, sha256sum)
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
FAILS=0
ok()  { echo "  OK    $*"; }
ko()  { echo "  ÉCHEC $*"; FAILS=$((FAILS + 1)); }
check() { if eval "$1"; then ok "$2"; else ko "$2"; fi; }

SECRET_ENV="POSTGRES_PASSWORD=fictif-mdp-7f3a9c"
SECRET_KEY="CLE-CA-FICTIVE cle-fictive-ca-91b2 (marqueur de test, aucune vraie cle)"

# ---- poste administrateur (hors serveur) : clé de récupération jetable
ADMIN_GNUPG="$T/admin-gnupg"; mkdir "$ADMIN_GNUPG"; chmod 700 "$ADMIN_GNUPG" 2>/dev/null || true
GNUPGHOME="$ADMIN_GNUPG" gpg --batch --quiet --passphrase '' \
  --quick-generate-key "Sauvegarde FICTIVE (test)" rsa3072 encr never 2>/dev/null
FPR="$(GNUPGHOME="$ADMIN_GNUPG" gpg --batch --with-colons --fingerprint | awk -F: '$1=="fpr"{print $10; exit}')"
GNUPGHOME="$ADMIN_GNUPG" gpg --batch --armor --export "$FPR" > "$T/backup-pubkey.asc"

# ---- données fictives et faux docker
mkdir -p "$T/fixture/secrets/cfg" "$T/fixture/secrets/caddy/caddy/pki/authorities/local" "$T/fixture/documents/dossiers"
printf '%s\n' "$SECRET_ENV" > "$T/fixture/secrets/cfg/.env"
printf '%s\n' "$SECRET_KEY" > "$T/fixture/secrets/caddy/caddy/pki/authorities/local/root.key"
printf 'PDF fictif\n' > "$T/fixture/documents/dossiers/v1-original.pdf"
mkdir -p "$T/bin"
cat > "$T/bin/docker" <<EOF
#!/usr/bin/env bash
case "\$*" in
  *pg_dump*)       printf 'DUMP-FICTIF\n' ;;
  *_documents:*)   cd "$T/fixture/documents" && tar czf - . ;;
  *_caddy_data:*)  cd "$T/fixture/secrets" && tar czf - cfg/.env caddy/caddy/pki ;;
  *) echo "docker fictif : commande inattendue \$*" >&2; exit 9 ;;
esac
EOF
chmod +x "$T/bin/docker"

run_backup() { # $1 = dossier destination, $2 = fichier de conf, reste = options
  local dest="$1" conf="$2"; shift 2
  PATH="$T/bin:$PATH" BACKUP_CONF="$conf" GNUPGHOME="$T/serveur-sans-cle" \
    bash "$REPO/deploy/backup.sh" "$dest" "$@" > "$T/sortie.txt" 2>&1
}
no_clear_secret() { ! grep -rqaF -e "fictif-mdp-7f3a9c" -e "cle-fictive-ca-91b2" "$1"; }

echo "1. Chiffrement configuré"
printf 'BACKUP_GPG_RECIPIENT=%s\nBACKUP_GPG_PUBKEY=%s\n' "$FPR" "$T/backup-pubkey.asc" > "$T/backup.conf"
set +e; run_backup "$T/out1" "$T/backup.conf"; code=$?; set -e
check '[ $code -eq 0 ]' "code de sortie 0 (complète)"
check 'grep -q "Sauvegarde COMPLÈTE" "$T/sortie.txt"' "annonce une sauvegarde complète"
check 'ls "$T"/out1/secrets-*.tar.gz.gpg >/dev/null 2>&1' "archive des secrets chiffrée présente"
check '! ls "$T"/out1/secrets-*.tar.gz >/dev/null 2>&1' "aucune archive des secrets en clair"
check '! ls "$T"/out1/*.partiel >/dev/null 2>&1' "aucun fichier intermédiaire"
check 'no_clear_secret "$T/out1"' "aucun secret fictif lisible dans les fichiers écrits"
check '(cd "$T/out1" && sha256sum -c --quiet sha256-*.txt)' "empreintes SHA-256 valides"
SEC="$(ls "$T"/out1/secrets-*.tar.gz.gpg)"
mkdir -p "$T/restaure" && GNUPGHOME="$ADMIN_GNUPG" gpg --batch --quiet --decrypt "$SEC" 2>/dev/null | (cd "$T/restaure" && tar xzf -)
check 'grep -qF "$SECRET_ENV" "$T/restaure/cfg/.env"' "déchiffrement (clé hors serveur) : .env restitué"
check 'grep -qF "cle-fictive-ca-91b2" "$T/restaure/caddy/caddy/pki/authorities/local/root.key"' "déchiffrement : clé de la CA restituée"
check '[ "$(GNUPGHOME="$ADMIN_GNUPG" gpg --batch --quiet --decrypt "$T"/out1/cellulo-*.dump.gpg 2>/dev/null)" = "DUMP-FICTIF" ]' "dump chiffré puis déchiffré à l'identique"
check '! GNUPGHOME="$T/autre" gpg --batch --quiet --decrypt "$SEC" >/dev/null 2>&1' "indéchiffrable sans la clé privée"

echo "2. Chiffrement NON configuré"
set +e; run_backup "$T/out2" "$T/absent.conf"; code=$?; set -e
check '[ $code -eq 3 ]' "code de sortie 3 (incomplète)"
check 'grep -q "SAUVEGARDE INCOMPLÈTE" "$T/sortie.txt"' "annonce une sauvegarde INCOMPLÈTE"
check '! grep -q "COMPLÈTE, chiffrée" "$T/sortie.txt"' "n'annonce pas une sauvegarde complète"
check '! ls "$T"/out2/secrets-* >/dev/null 2>&1' "aucune archive des secrets créée"
check 'no_clear_secret "$T/out2"' "aucun secret fictif écrit"

echo "3. Empreinte incohérente"
printf 'BACKUP_GPG_RECIPIENT=%s\nBACKUP_GPG_PUBKEY=%s\n' "0000000000000000000000000000000000000000" "$T/backup-pubkey.asc" > "$T/faux.conf"
set +e; run_backup "$T/out3" "$T/faux.conf"; code=$?; set -e
check '[ $code -ne 0 ] && [ $code -ne 3 ]' "refus (empreinte ≠ clé publique)"
check '! ls "$T"/out3/* >/dev/null 2>&1' "rien n'est écrit"

echo "4. --sans-secrets (choix explicite)"
set +e; run_backup "$T/out4" "$T/backup.conf" --sans-secrets; code=$?; set -e
check '[ $code -eq 0 ]' "code de sortie 0"
check 'grep -q "PARTIELLE" "$T/sortie.txt"' "annonce une sauvegarde partielle"
check '! ls "$T"/out4/secrets-* >/dev/null 2>&1' "aucune archive des secrets"

echo
if [ "$FAILS" -eq 0 ]; then echo "test-backup : tous les contrôles réussis"; else echo "test-backup : $FAILS contrôle(s) en échec"; exit 1; fi
