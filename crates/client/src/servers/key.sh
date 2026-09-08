set -eu
umask 077
test ! -L "$HOME/.ssh"
mkdir -p "$HOME/.ssh"
test "$(stat -c %u "$HOME/.ssh")" = "$(id -u)"
test ! -L "$HOME/.ssh/authorized_keys"
exec 9> "$HOME/.ssh/.remote-codex-key.lock"
flock -w 15 9 || exit 75
RC_KEYS="$HOME/.ssh/authorized_keys"
if test ! -e "$RC_KEYS"; then touch "$RC_KEYS"; fi
test -f "$RC_KEYS"
test "$(stat -c %u "$RC_KEYS")" = "$(id -u)"
if test "$RC_REVOKE" = 1; then
    RC_TEMP=$(mktemp "$HOME/.ssh/.remote-codex-key-XXXXXX")
    trap 'rm -f -- "$RC_TEMP"' EXIT
    grep -Fvx -- "$RC_PUBLIC" "$RC_KEYS" > "$RC_TEMP" || test "$?" = 1
    chmod --reference="$RC_KEYS" "$RC_TEMP"
    mv -f -- "$RC_TEMP" "$RC_KEYS"
else
    if ! grep -Fx -- "$RC_PUBLIC" "$RC_KEYS" >/dev/null; then
        printf '\n%s\n' "$RC_PUBLIC" >> "$RC_KEYS"
    fi
fi
