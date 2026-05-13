#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "Building with debug symbols..."
cargo build -p sizzle-gtk 2>&1

echo "Running under valgrind..."
exec valgrind ./target/debug/sizzle-gtk "$@"
