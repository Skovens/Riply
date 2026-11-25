#!/usr/bin/env bash
# Wrapper that ensures Delve can find the Go toolchain when spawned by the DAP agent.
# The agent may run with a minimal PATH, but dlv needs `go` to build package-mode targets.
set -euo pipefail

export PATH="$PATH:/usr/local/go/bin:/opt/go/bin:$HOME/go/bin:$HOME/.local/go/bin:$HOME/.local/bin"

if ! command -v dlv >/dev/null 2>&1; then
    echo "dlv-wrapper: dlv not found in PATH=$PATH" >&2
    exit 1
fi

if ! command -v go >/dev/null 2>&1; then
    echo "dlv-wrapper: go not found in PATH=$PATH" >&2
    exit 1
fi

exec dlv "$@"
