#!/usr/bin/env bash
# System packages the Linux jobs need (Debian/Ubuntu). On GitHub's runners and on a
# self-hosted runner with passwordless sudo (or running as root) the missing ones are
# installed; otherwise the job stops with the command to run once as root on the runner.
set -euo pipefail

packages=(
  build-essential pkg-config curl file git xdg-utils patchelf
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev
  musl-tools
)

missing=()
for p in "${packages[@]}"; do
  dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "install ok installed" || missing+=("$p")
done
if [ ${#missing[@]} -eq 0 ]; then
  echo "All system packages are installed."
  exit 0
fi

echo "Missing system packages: ${missing[*]}"
if [ "$(id -u)" -eq 0 ]; then
  apt-get update && apt-get install -y "${missing[@]}"
elif sudo -n true 2>/dev/null; then
  sudo apt-get update && sudo apt-get install -y "${missing[@]}"
else
  echo "::error::The runner cannot install packages (no root, no passwordless sudo). Run this once as root on the runner, then re-run the job:"
  echo "apt-get update && apt-get install -y ${missing[*]}"
  exit 1
fi
