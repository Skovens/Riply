#!/usr/bin/env bash
# install-makemkv.sh — wrapper for humans running the installer by hand.
#
# The canonical installer lives in the Go package (the riply binary embeds
# the same file, so the two can never drift). This keeps scripts/ as the
# stable path for `bash scripts/install-makemkv.sh` in docs and the
# in-app prompt.
exec bash "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/internal/makemkvinstall/install-makemkv.sh" "$@"
