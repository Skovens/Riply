#!/usr/bin/env bash
set -e

# Rebuild Rust (fast if nothing changed)
echo "Building Rust..."
cargo build -p cli --quiet

# Rebuild frontend (fast, ~0.5s)
echo "Building frontend..."
cd web/svelte
npm run build --silent

# Start server
echo "Starting server..."
exec cargo run -p cli -- serve
