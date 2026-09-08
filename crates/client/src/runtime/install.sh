set -eu
umask 077
PATH=/usr/local/bin:/usr/bin:/bin
export PATH
test ! -L "$RC_ROOT"
test "$(stat -c %a "$RC_ROOT")" = 700
for RC_DIR in "$RC_ROOT/locks" "$RC_ROOT/runtimes" "$RC_ROOT/runtimes/codex"; do
    test ! -L "$RC_DIR"
    mkdir -p "$RC_DIR"
    test "$(stat -c %a "$RC_DIR")" = 700
done
exec 9> "$RC_ROOT/locks/codex-$RC_VERSION.lock"
flock -w 15 9 || { echo 'another runtime installation is active' >&2; exit 75; }
RC_TARGET="$RC_ROOT/runtimes/codex/$RC_VERSION-linux-x86_64"
find "$RC_ROOT/runtimes/codex" -maxdepth 1 -type d -name ".staging-$RC_VERSION-*" -exec rm -rf -- {} +
RC_STAGE=$(mktemp -d "$RC_ROOT/runtimes/codex/.staging-$RC_VERSION-XXXXXX")
trap 'rm -rf -- "$RC_STAGE"; rm -f -- "$RC_ARCHIVE" "$RC_ROOT/.hashes-$RC_OPERATION"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
test ! -L "$RC_ARCHIVE"
printf '%s  %s\n' "$RC_SHA256" "$RC_ARCHIVE" | sha256sum --check --status
if test -e "$RC_TARGET"; then
    test ! -L "$RC_TARGET"
    test "$(cat "$RC_TARGET/.archive-sha256")" = "$RC_SHA256"
    (cd "$RC_TARGET" && sha256sum --check --quiet .files-sha256)
    printf 'REMOTE_CODEX_RUNTIME_REUSED_V1\n'
    exit 0
fi
tar -tzf "$RC_ARCHIVE" > "$RC_STAGE/.archive-list"
if grep -E '(^/|(^|/)\.\.(/|$))' "$RC_STAGE/.archive-list" >/dev/null; then exit 70; fi
rm "$RC_STAGE/.archive-list"
tar --no-same-owner --no-same-permissions -xzf "$RC_ARCHIVE" -C "$RC_STAGE"
test -x "$RC_STAGE/bin/codex"
test -x "$RC_STAGE/bin/codex-code-mode-host"
test -x "$RC_STAGE/codex-path/rg"
test -x "$RC_STAGE/codex-resources/bwrap"
RC_CHECK=$(mktemp -d "$RC_STAGE/.health-XXXXXX")
RC_ACTUAL=$(CODEX_HOME="$RC_CHECK" "$RC_STAGE/bin/codex" --version)
test "$RC_ACTUAL" = "codex-cli $RC_VERSION"
rm -rf -- "$RC_CHECK"
(cd "$RC_STAGE" && find . -type f -print0 | sort -z | xargs -0 sha256sum) > "$RC_ROOT/.hashes-$RC_OPERATION"
mv "$RC_ROOT/.hashes-$RC_OPERATION" "$RC_STAGE/.files-sha256"
printf '%s\n' "$RC_SHA256" > "$RC_STAGE/.archive-sha256"
sync -f "$RC_STAGE"
mv -T "$RC_STAGE" "$RC_TARGET"
sync -f "$RC_ROOT/runtimes/codex"
printf 'REMOTE_CODEX_RUNTIME_READY_V1\n'
