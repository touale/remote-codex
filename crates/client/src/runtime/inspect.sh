set -eu
umask 077
PATH=/usr/local/bin:/usr/bin:/bin
export PATH
case "$RC_ROOT" in /*) ;; *) exit 70;; esac
test ! -L "$RC_ROOT"
mkdir -p "$RC_ROOT"
test "$(stat -c %a "$RC_ROOT")" = 700 || { echo 'remote-codex data directory must have mode 700' >&2; exit 70; }
# Uploads are bounded to five minutes. Reclaim only abandoned product temporary files.
find "$RC_ROOT" -maxdepth 1 -type f \( -name '.incoming-*.tar.gz' -o -name '.hashes-*' \) -mmin +60 -delete
for RC_DIR in "$RC_ROOT/runtimes" "$RC_ROOT/runtimes/codex"; do
    test ! -L "$RC_DIR"
    mkdir -p "$RC_DIR"
    test "$(stat -c %a "$RC_DIR")" = 700
done
RC_TARGET="$RC_ROOT/runtimes/codex/$RC_VERSION-linux-x86_64"
if test -d "$RC_TARGET"; then
    test ! -L "$RC_TARGET"
    test "$(cat "$RC_TARGET/.archive-sha256")" = "$RC_SHA256"
    (cd "$RC_TARGET" && sha256sum --check --quiet .files-sha256)
    RC_CHECK=$(mktemp -d "$RC_ROOT/.health-XXXXXX")
    trap 'rm -rf -- "$RC_CHECK"' EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    RC_ACTUAL=$(CODEX_HOME="$RC_CHECK" "$RC_TARGET/bin/codex" --version)
    test "$RC_ACTUAL" = "codex-cli $RC_VERSION"
    printf 'REMOTE_CODEX_RUNTIME_READY_V1\n'
else
    printf 'REMOTE_CODEX_RUNTIME_MISSING_V1\n'
fi
