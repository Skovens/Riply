#!/usr/bin/env bash
set -e

# Rebuild frontend (fast, ~0.5s)
echo "Building frontend..."
cd web/svelte
npm run build --silent

# Start server
echo "Starting server..."
exec cargo run -p cli -- serve
