#!/bin/sh
# Deliberately tiny fixture for numeric-ID requests from the Codex client.
# Only exposes fixed read-only identity data; never executes client arguments.
set -eu
while IFS= read -r request; do
    id=$(printf '%s' "$request" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p')
    [ -n "$id" ] || continue
    case "$request" in
        *'"initialize"'*)
            result='{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"execution-probe","version":"1"},"instructions":"Synthetic read-only execution identity fixture."}'
            ;;
        *'"tools/list"'*)
            result='{"tools":[{"name":"probe_identity","description":"Read operating system and configured fixture marker.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}]}'
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
