RC_WAIT=0
while test ! -f "$RC_ROOT/owner" && test "$RC_WAIT" -lt 20; do
    /bin/sleep 0.05
    RC_WAIT=$((RC_WAIT + 1))
done
test "$PPID" = "$(/bin/cat "$RC_ROOT/owner")" || exit 1
{
    printf '%s\n' "$RC_TOKEN"
    printf '%s' "$1" | /usr/bin/base64 | /usr/bin/tr -d '\n'
    printf '\n'
} | /usr/bin/nc -w 180 -U "$RC_ROOT/prompt"
