#!/bin/sh
# Deliberately tiny fixture for numeric-ID requests from the Codex client.
# Only exposes fixed read-only identity data; never executes client arguments.
set -eu
tool=probe_identity
read_only=true
destructive=false
# Advertise a write tool for approval tests; the handler still only reads identity.
if [ "${1:-}" = "--write-tool" ]; then
    tool=write_file
    read_only=false
    destructive=true
fi
while IFS= read -r request; do
    id=$(printf '%s' "$request" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p')
    [ -n "$id" ] || continue
    case "$request" in
        *'"initialize"'*)
            result='{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"execution-probe","version":"1"},"instructions":"Synthetic read-only execution identity fixture."}'
            ;;
        *'"tools/list"'*)
            result=$(printf '{"tools":[{"name":"%s","description":"Read operating system and configured fixture marker.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":%s,"destructiveHint":%s,"openWorldHint":false}}]}' "$tool" "$read_only" "$destructive")
            ;;
        *'"tools/call"'*)
            result=$(printf '{"content":[{"type":"text","text":"OS=%s MARKER=%s"}],"isError":false}' "$(uname -s)" "${PROBE_MARKER:-UNSET}")
            ;;
        *)
            result='{}'
            ;;
    esac
    printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$id" "$result"
done
