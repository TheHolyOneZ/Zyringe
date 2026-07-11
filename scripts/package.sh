#!/usr/bin/env bash
# Zyringe release packager — builds the GUI bundles (AppImage + .deb) plus the
# privileged pieces, and collects everything into ./dist/ ready to ship/install.
#
#   bash scripts/package.sh
#
# Output (./dist/):
#   Zyringe_<ver>_amd64.AppImage   portable GUI
#   Zyringe_<ver>_amd64.deb        GUI as a Debian/Ubuntu package
#   zyringe-inject                 privileged ptrace bin (release)
#   libzyringe.so                  in-target helper payload
#   install.sh  49-zyringe.rules   privileged-bits installer + polkit rule
#   INSTALL.txt                    what to run after
#
# NOTE: the GUI bundle is unprivileged. Injection needs the privileged bits
# installed once via `sudo bash install.sh` (polkit-pinned to /usr/lib/zyringe).
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
ROOT="$PWD"

die() { echo; echo "############################################"; \
        echo "##  PACKAGE FAILED: $1"; \
        echo "############################################"; exit 1; }

echo "==> [1/6] app icons"
[ -f src-tauri/icons/icon.png ] || die "src-tauri/icons/icon.png missing — generate icons from your own logo:  pnpm tauri icon path/to/logo.png"

echo "==> [2/6] helper payload (libzyringe.so)"
gcc -shared -fPIC -Wall -Wextra -Wno-unused-parameter -O2 \
    -o helper/libzyringe.so helper/zyringe_helper.c -ldl -lpthread || die "helper .so"

echo "==> [3/6] privileged injector (release)"
cargo build --release -p zyringe-inject || die "zyringe-inject"

echo "==> [4/6] frontend deps"
pnpm install || die "pnpm install"

echo "==> [5/6] tauri build (AppImage + .deb)"
pnpm tauri build || die "tauri build (see errors above)"

echo "==> [6/6] collecting ./dist/"
DIST="$ROOT/dist"
rm -rf "$DIST"; mkdir -p "$DIST"

BUNDLE="$ROOT/src-tauri/target/release/bundle"
found=0
for f in "$BUNDLE"/appimage/*.AppImage "$BUNDLE"/deb/*.deb; do
  [ -f "$f" ] && cp -v "$f" "$DIST/" && found=1
done
[ "$found" = 1 ] || die "no AppImage/.deb produced under $BUNDLE"

cp -v target/release/zyringe-inject "$DIST/"
cp -v helper/libzyringe.so          "$DIST/"
cp -v scripts/install.sh            "$DIST/"
[ -f packaging/49-zyringe.rules ] && cp -v packaging/49-zyringe.rules "$DIST/"

cat > "$DIST/INSTALL.txt" <<'TXT'
Zyringe — install
=================

1. Install the GUI:
     - AppImage:  chmod +x Zyringe_*.AppImage && ./Zyringe_*.AppImage
     - or .deb:   sudo apt install ./Zyringe_*.deb   (or: sudo dpkg -i)

2. Install the privileged injection bits ONCE (needed for Attach/Launch):
     sudo bash install.sh
   This puts zyringe-inject + libzyringe.so under /usr/lib/zyringe (root-owned)
   and a polkit rule that pins that exact path. Injection then uses pkexec.

   Remove with:
     sudo rm -rf /usr/lib/zyringe /etc/polkit-1/rules.d/49-zyringe.rules

Note: `install.sh` looks for zyringe-inject at ../target/release — when running
from ./dist run it as:  sudo bash install.sh   (it also accepts the local copies
here). See install.sh header if paths differ.
TXT

echo
echo "############################################"
echo "##  PACKAGE OK  ->  $DIST"
ls -1 "$DIST"
echo "############################################"
