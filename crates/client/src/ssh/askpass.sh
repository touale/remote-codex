set -eu
# Only the owned SSH master may request this server's password. A ProxyJump
# child must authenticate with its own key/agent instead of receiving this secret.
RC_OWNER_WAIT=0
while test ! -f "$RC_ASKPASS_ROOT/owner" && test "$RC_OWNER_WAIT" -lt 20; do
    /bin/sleep 0.05
    RC_OWNER_WAIT=$((RC_OWNER_WAIT + 1))
done
test "$PPID" = "$(/bin/cat "$RC_ASKPASS_ROOT/owner")" || exit 1
if test "$1" = "$RC_ASKPASS_PASSWORD_PROMPT"; then
    if printf '%s\n' "$RC_ASKPASS_TOKEN" | /usr/bin/nc -w 5 -U "$RC_ASKPASS_ROOT/password"; then
        exit 0
    fi
    : > "$RC_ASKPASS_ROOT/vault-error"
    exit 1
fi
# Host identity confirmation is always a human decision. Passwords are never
# returned for host-key prompts, key passphrases, MFA or password-change requests.
case "$1" in
    *'Are you sure you want to continue connecting (yes/no/[fingerprint])? ')
        test "$RC_ASKPASS_INTERACTIVE" = 1 || exit 1
        printf '%s' "$1" > /dev/tty
        IFS= read -r RC_HOST_ANSWER < /dev/tty || exit 1
        printf '%s\n' "$RC_HOST_ANSWER"
        ;;
    *) exit 1 ;;
esac
