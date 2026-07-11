#!/usr/bin/env bash
# Zyringe one-shot: build EVERYTHING, then launch the app.
#
#   bash scripts/dev.sh          build + run (default)
#   bash scripts/dev.sh --build  build only, don't launch (just show errors)
#
# If any build step fails you'll see a big FAILED banner and it stops there.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

die() { echo; echo "############################################"; \
        echo "##  BUILD FAILED: $1"; \
        echo "############################################"; exit 1; }

echo "==> [1/4] app icons"
[ -f src-tauri/icons/icon.png ] || die "src-tauri/icons/icon.png missing — generate icons from your own logo:  pnpm tauri icon path/to/logo.png"

echo "==> [2/4] helper payload (libzyringe.so)"
gcc -shared -fPIC -Wall -Wextra -Wno-unused-parameter -O2 \
    -o helper/libzyringe.so helper/zyringe_helper.c -ldl -lpthread \
    || die "helper .so (gcc)"
readelf -d helper/libzyringe.so 2>/dev/null | grep NEEDED || true
echo "    helper OK"

echo "==> [3/4] privileged injector (zyringe-inject)"
cargo build -p zyringe-inject || die "zyringe-inject (cargo)"
echo "    zyringe-inject OK"

echo "==> [3b] frontend deps"
pnpm install || die "pnpm install"

if [ "${1:-}" = "--build" ]; then
  echo
  echo "==> BUILD OK (skipping launch, --build given)"
  exit 0
fi

echo "==> [4/4] launching app  (Ctrl-C to stop)"
echo "    NOTE: if it errors with 'port 1420 in use', an old dev server is"
echo "          still running — close it first, then re-run this script."
exec pnpm tauri dev
