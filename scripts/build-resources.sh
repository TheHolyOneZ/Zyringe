#!/usr/bin/env bash
# Build the native pieces and stage them where Tauri expects bundle resources.
#   - helper/libzyringe.so        (C payload)
#   - target/release/zyringe-inject (privileged ptrace bin)
# Copies both into src-tauri/resources/ for AppImage bundling.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Building helper .so"
make -C helper

echo "==> Building zyringe-inject (release)"
cargo build --release -p zyringe-inject

echo "==> Staging resources"
mkdir -p src-tauri/resources
cp helper/libzyringe.so       src-tauri/resources/libzyringe.so
cp target/release/zyringe-inject src-tauri/resources/zyringe-inject
chmod +x src-tauri/resources/zyringe-inject

echo "==> Done. Resources staged in src-tauri/resources/"
