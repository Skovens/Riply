#!/usr/bin/env bash
# Riply debug launcher.
# Prefer the DAP debugger via xd://debug for agentic debugging.
# This script is a manual fallback for headless/local Delve sessions.
set -euo pipefail

cd "$(dirname "$0")/.."

usage() {
    cat <<EOF
Usage: $0 {launch|attach|headless} [PID]

  launch   Start riply under Delve (interactive)
  attach   Attach Delve to a running riply process (requires PID)
  headless Start riply under Delve in headless DAP mode on 127.0.0.1:40000

Examples:
  $0 launch
  $0 attach 12345
  $0 headless
EOF
    exit 1
}

# Ensure common Go installation directories are on PATH for Delve builds.
if ! command -v go >/dev/null 2>&1; then
    export PATH="$PATH:/usr/local/go/bin:/opt/go/bin:$HOME/go/bin:$HOME/.local/go/bin"
fi

if ! command -v dlv >/dev/null 2>&1; then
    echo "Delve not found. Install with: go install github.com/go-delve/delve/cmd/dlv@latest"
    exit 1
fi

mode="${1:-}"
pid="${2:-}"

case "$mode" in
    launch)
        exec dlv debug ./cmd/server
        ;;
    attach)
        if [[ -z "$pid" || ! "$pid" =~ ^[0-9]+$ ]]; then
            echo "Error: attach requires a numeric PID"
            usage
        fi
        exec dlv attach "$pid"
        ;;
    headless)
        exec dlv debug ./cmd/server --headless --listen=127.0.0.1:40000 --api-version=2
        ;;
    *)
        usage
        ;;
esac
