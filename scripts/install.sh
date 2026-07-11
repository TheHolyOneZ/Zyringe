#!/usr/bin/env bash
# System install for Zyringe's privileged pieces, hardened against privesc.
#
# The polkit rule (packaging/49-zyringe.rules) pins the EXACT path
# /usr/lib/zyringe/zyringe-inject. This script installs the binary there
# root-owned and NOT user-writable, so a local user cannot swap the privileged
# binary that pkexec will run as root. That pinning is the whole point.
#
# Usage:
#   bash scripts/build-resources.sh        # build helper + zyringe-inject (as you)
#   sudo bash scripts/install.sh           # install privileged bits (as root)
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "Run as root:  sudo bash scripts/install.sh"
  exit 1
fi

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

# Prefer copies sitting next to this script (the shipped ./dist/), else fall
# back to the repo build tree — so this works both packaged and from source.
pick() { for c in "$@"; do [ -f "$c" ] && { echo "$c"; return; }; done; echo "$1"; }
HELPER="$(pick "$HERE/libzyringe.so"      "$ROOT/helper/libzyringe.so")"
INJECT="$(pick "$HERE/zyringe-inject"     "$ROOT/target/release/zyringe-inject")"
RULES="$(pick  "$HERE/49-zyringe.rules"   "$ROOT/packaging/49-zyringe.rules")"

[ -f "$HELPER" ] || { echo "missing libzyringe.so  — run: gcc … helper, or scripts/build-resources.sh"; exit 1; }
[ -f "$INJECT" ] || { echo "missing zyringe-inject  — run: cargo build --release -p zyringe-inject"; exit 1; }
[ -f "$RULES"  ] || { echo "missing 49-zyringe.rules (packaging/)"; exit 1; }

install -d -o root -g root -m 0755 /usr/lib/zyringe
install -o root -g root -m 0755 "$INJECT" /usr/lib/zyringe/zyringe-inject
install -o root -g root -m 0644 "$HELPER" /usr/lib/zyringe/libzyringe.so
install -o root -g root -m 0644 "$RULES"  /etc/polkit-1/rules.d/49-zyringe.rules

echo "Installed (root-owned, polkit-pinned):"
echo "  /usr/lib/zyringe/zyringe-inject   0755 root:root"
echo "  /usr/lib/zyringe/libzyringe.so    0644 root:root"
echo "  /etc/polkit-1/rules.d/49-zyringe.rules"
echo
echo "Zyringe will now prefer these paths automatically. Remove with:"
echo "  sudo rm -rf /usr/lib/zyringe /etc/polkit-1/rules.d/49-zyringe.rules"
