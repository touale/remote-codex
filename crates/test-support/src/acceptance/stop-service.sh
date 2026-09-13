set -eu
case "$RC_ROOT" in /tmp/remote-codex-acceptance-*/service) ;; *) exit 2 ;; esac
for RC_PROC in /proc/[0-9]*/cmdline; do
    test -r "$RC_PROC" || continue
    RC_ARGS=$(tr '\000' '\n' < "$RC_PROC" 2>/dev/null) || continue
    test "$(printf '%s\n' "$RC_ARGS" | tail -n 1)" = serve || continue
    printf '%s\n' "$RC_ARGS" | grep -Fx -- "$RC_ROOT" >/dev/null || continue
    case "$RC_ARGS" in *remote-codex-server*) ;; *) continue ;; esac
    test "$(stat -c %u "$RC_PROC")" = "$(id -u)" || exit 2
    RC_PID=${RC_PROC%/cmdline}
    RC_PID=${RC_PID##*/}
    kill -TERM "$RC_PID"
    RC_WAIT=0
    while test -r "/proc/$RC_PID/stat"; do
        test "$(awk '{print $3}' "/proc/$RC_PID/stat")" != Z || break
        RC_WAIT=$((RC_WAIT + 1))
        test "$RC_WAIT" -lt 12 || exit 3
        sleep 1
    done
done
