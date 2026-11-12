#!/usr/bin/env bash
# Test de deploy/restore-isole.sh avec un faux « docker » (journal des
# appels) et un faux « id » (root simulé). Données FICTIVES. Vérifie les
# garde-fous, l'ordre des étapes, l'arrêt avant démarrage de l'application,
# la conservation des fichiers en cas d'échec et « -p cellulo-restore »
# sur chaque commande compose. Ne remplace PAS la recette sur une vraie VM.
# Usage : bash deploy/tests/test-restore.sh   (nécessite openssl, tar)
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
FAILS=0
ok()  { echo "  OK    $*"; }
ko()  { echo "  ÉCHEC $*"; FAILS=$((FAILS + 1)); }
check() { if eval "$1"; then ok "$2"; else ko "$2"; fi; }

FIX="$T/fix"; mkdir -p "$FIX" "$T/bin"
# Chemins relatifs + MSYS_NO_PATHCONV : identique sous Linux et Git Bash.
( cd "$FIX" \
  && MSYS_NO_PATHCONV=1 openssl req -x509 -newkey rsa:2048 -nodes -subj "/CN=Caddy Local Authority FICTIVE" \
       -days 2 -keyout root.key -out root.crt >/dev/null 2>&1 \
  && MSYS_NO_PATHCONV=1 openssl req -x509 -newkey rsa:2048 -nodes -subj "/CN=Autre autorite FICTIVE" \
       -days 2 -keyout other.key -out other.crt >/dev/null 2>&1 )
CA_FPR="$(openssl x509 -noout -fingerprint -sha256 -in "$FIX/root.crt" | cut -d= -f2 | tr -d ':')"
printf '%s  %s\n' "$(printf 'pdf' | sha256sum | cut -d' ' -f1)" "dossiers/ONB-00001/remises/MVT-00001/v1-original.pdf" \
  "$(printf 'scan' | sha256sum | cut -d' ' -f1)" "dossiers/ONB-00001/remises/MVT-00001/v1-scan-1.pdf" > "$FIX/docs.txt"

make_source() { # $1 = dossier
  local s="$1" w="$T/w$RANDOM"
  mkdir -p "$s" "$w/cfg" "$w/caddy/caddy/pki/authorities/local"
  printf 'SERVER_ADDRESS=10.9.9.9\nPOSTGRES_PASSWORD=fictif\n' > "$w/cfg/.env"
  cp "$FIX/root.crt" "$w/caddy/caddy/pki/authorities/local/root.crt"
  (cd "$w" && tar czf - cfg caddy) > "$s/secrets.tar.gz"
  printf 'DUMP-FICTIF' > "$s/cellulo.dump"
  printf 'DOCS-FICTIFS' | gzip > "$s/documents.tar.gz"
}

cat > "$T/bin/id" <<'EOF'
#!/bin/sh
[ "$1" = "-u" ] && { echo 0; exit 0; }
exec /usr/bin/id "$@"
EOF
cat > "$T/bin/docker" <<EOF
#!/usr/bin/env bash
echo "\$*" >> "\$LOG"
a="\$*"
case "\$a" in
  "ps -a -q --filter label=com.docker.compose.project=celluloplast") if [ "\${MOCK_PROD:-0}" = 1 ]; then echo abc123; fi ;;
  "volume ls -q --filter label=com.docker.compose.project=celluloplast") : ;;
  "ps -a -q --filter label=com.docker.compose.project=cellulo-restore") if [ "\${MOCK_CIBLE:-0}" = 1 ]; then echo def456; fi ;;
  "volume ls -q --filter label=com.docker.compose.project=cellulo-restore") : ;;
  "volume create "*) : ;;
  *"run --rm -v cellulo-restore_documents:/data -v "*) : ;;
  *"run --rm -v cellulo-restore_caddy_data:/caddy -v "*) : ;;
  *"run --rm -v cellulo-restore_caddy_data:/caddy:ro"*) if [ "\${MOCK_CA_CHANGED:-0}" = 1 ]; then cat "$FIX/other.crt"; else cat "$FIX/root.crt"; fi ;;
  *"run -i --rm -v cellulo-restore_documents:/data:ro"*) while read -r h p; do echo "\$p: OK"; done ;;
  *" up -d db") : ;;
  *pg_isready*) : ;;
  *"current_database()"*) echo "cellulo|\${MOCK_TABLES:-0}" ;;
  *pg_restore*) cat > /dev/null; if [ "\${MOCK_RESTORE_FAIL:-0}" = 1 ]; then exit 1; fi ;;
  *" up -d --build") : ;;
  *"count(*) from cases"*) echo 2 ;;
  *"string_agg(reference"*) echo "DEM-00001, ONB-00001" ;;
  *user_roles*) echo "      it.formation.demo@exemple.test : FORMATION + IT" ;;
  *document_versions*) cat "$FIX/docs.txt" ;;
  *"printenv MAIL_MODE"*) echo journal ;;
  *deliveries*) echo "journalise=4" ;;
  *) echo "docker fictif : appel inattendu : \$a" >&2; exit 9 ;;
esac
EOF
chmod +x "$T/bin/docker" "$T/bin/id"

N=0
run() { # options du script ; variables MOCK_* héritées
  N=$((N + 1)); export LOG="$T/log$N"; : > "$LOG"
  SRC="$T/src$N"; make_source "$SRC"
  set +e
  PATH="$T/bin:$PATH" RESTORE_ENV_FILE="$T/restore$N.env" bash "$REPO/deploy/restore-isole.sh" --source "$SRC" "$@" > "$T/out$N" 2>&1
  CODE=$?
  set -e
  OUT="$T/out$N"
}
started() { grep -q -- " up -d --build" "$LOG"; }

echo "1. Recette nominale"
run --adresse 192.168.50.10 --machine-de-test-confirmee --empreinte-ca-attendue "$CA_FPR"
check '[ $CODE -eq 0 ]' "code 0"
check 'grep -q "RECETTE DE RESTAURATION RÉUSSIE" "$OUT"' "recette réussie"
check 'grep -q "identique avant/après" "$OUT"' "certificat racine identique avant/après"
check 'grep -q "2/2 conformes" "$OUT"' "empreintes PDF et scan conformes"
check '[ "$(grep -n pg_restore "$LOG" | cut -d: -f1)" -lt "$(grep -n -- " up -d --build" "$LOG" | cut -d: -f1)" ]' "application démarrée APRÈS pg_restore"
check '! grep "^compose" "$LOG" | grep -vq -- "-p cellulo-restore"' "chaque commande compose porte -p cellulo-restore"
check '! grep -q "celluloplast_" "$LOG"' "aucun volume de production touché"
check '[ -f "$SRC/cellulo.dump" ]' "fichiers source conservés sans --supprimer-source"
check '[ -f "$T/restore$N.env" ] && grep -q "^SERVER_ADDRESS=192.168.50.10$" "$T/restore$N.env" && ! grep -q "fictif" "$T/restore$N.env"' "environnement de recette conservé, adresse de la VM, aucun secret de production repris"
check '! grep "^compose" "$LOG" | grep -vq -- "--env-file $T/restore$N.env"' "chaque commande compose utilise cet environnement"

echo "2. Sans confirmation « machine de test »"
run --adresse 192.168.50.10
check '[ $CODE -ne 0 ] && [ ! -s "$LOG" ]' "refus avant tout appel docker"

echo "3. Production présente sur la machine"
MOCK_PROD=1 run --adresse 192.168.50.10 --machine-de-test-confirmee
check '[ $CODE -ne 0 ] && grep -q "PRODUCTION" "$OUT" && ! started' "refus, rien démarré"

echo "4. Projet de recette déjà présent (cible non vide)"
MOCK_CIBLE=1 run --adresse 192.168.50.10 --machine-de-test-confirmee
check '[ $CODE -ne 0 ] && grep -q "cible non vide" "$OUT" && ! started' "refus, rien démarré"

echo "5. Adresse de production utilisée pour la recette"
run --adresse 10.9.9.9 --machine-de-test-confirmee
check '[ $CODE -ne 0 ] && grep -q "adresse de PRODUCTION" "$OUT" && ! grep -q "volume create" "$LOG"' "refus avant création des volumes"

echo "6. Empreinte de CA inattendue"
run --adresse 192.168.50.10 --machine-de-test-confirmee --empreinte-ca-attendue 00AA
check '[ $CODE -ne 0 ] && ! grep -q "volume create" "$LOG"' "refus avant création des volumes"

echo "7. Base cible non vide"
MOCK_TABLES=12 run --adresse 192.168.50.10 --machine-de-test-confirmee
check '[ $CODE -ne 0 ] && grep -q "base cible non vide" "$OUT"' "refus détecté"
check '! grep -q pg_restore "$LOG" && ! started' "ni pg_restore ni démarrage"

echo "8. pg_restore en échec"
MOCK_RESTORE_FAIL=1 run --adresse 192.168.50.10 --machine-de-test-confirmee
WORKDIR="$(sed -n 's/.*espace de travail : \(.*\) (conservé.*/\1/p' "$OUT")"
check '[ $CODE -ne 0 ] && grep -q "pg_restore en échec" "$OUT"' "arrêt sur échec"
check '! started' "application NON démarrée"
check '[ -n "$WORKDIR" ] && [ -d "$WORKDIR" ] && [ -f "$SRC/cellulo.dump" ]' "fichiers conservés (pas de nettoyage)"
rm -rf "$WORKDIR"

echo "9. CA régénérée au démarrage"
MOCK_CA_CHANGED=1 run --adresse 192.168.50.10 --machine-de-test-confirmee
check '[ $CODE -ne 0 ] && grep -q "DIFFÉRENT" "$OUT" && grep -q "RECETTE EN ÉCHEC" "$OUT"' "écart signalé, recette en échec"
WORKDIR="$(sed -n 's/.*espace de travail : \(.*\) (conservé.*/\1/p' "$OUT")"; rm -rf "$WORKDIR"

echo
if [ "$FAILS" -eq 0 ]; then echo "test-restore : tous les contrôles réussis"; else echo "test-restore : $FAILS contrôle(s) en échec"; exit 1; fi
