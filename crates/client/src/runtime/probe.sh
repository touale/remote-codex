set -eu
PATH=/usr/local/bin:/usr/bin:/bin
export PATH
test -n "${HOME:-}"
printf 'REMOTE_CODEX_HOST_V1\n%s\n%s\n%s\n' "$(uname -s)" "$(uname -m)" "$HOME"
