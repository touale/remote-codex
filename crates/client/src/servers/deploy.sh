set -eu
umask 077
PATH=/usr/local/bin:/usr/bin:/bin
export PATH
test ! -L "$RC_ROOT"
test "$(stat -c %a "$RC_ROOT")" = 700
for RC_DIR in "$RC_ROOT/locks" "$RC_ROOT/runtimes" "$RC_ROOT/runtimes/service"; do
    test ! -L "$RC_DIR"
    mkdir -p "$RC_DIR"
    test "$(stat -c %a "$RC_DIR")" = 700
done
exec 9> "$RC_ROOT/locks/service-install.lock"
flock -w 15 9 || exit 75
trap 'rm -f -- "$RC_INCOMING"' EXIT
test ! -L "$RC_INCOMING"
printf '%s  %s\n' "$RC_HASH" "$RC_INCOMING" | sha256sum --check --status
RC_TARGET="$RC_ROOT/runtimes/service/$RC_HASH"
test ! -L "$RC_TARGET"
if test -d "$RC_TARGET"; then
    printf '%s  %s\n' "$RC_HASH" "$RC_TARGET/remote-codex-server" | sha256sum --check --status
else
    RC_STAGE=$(mktemp -d "$RC_ROOT/runtimes/service/.stage-XXXXXX")
    trap 'rm -rf -- "$RC_STAGE"; rm -f -- "$RC_INCOMING"' EXIT
    cp -- "$RC_INCOMING" "$RC_STAGE/remote-codex-server"
    chmod 700 "$RC_STAGE/remote-codex-server"
    "$RC_STAGE/remote-codex-server" --version >/dev/null
    sync -f "$RC_STAGE"
    mv -T "$RC_STAGE" "$RC_TARGET"
    sync -f "$RC_ROOT/runtimes/service"
fi
printf 'REMOTE_CODEX_SERVICE_INSTALLED_V1\n'
