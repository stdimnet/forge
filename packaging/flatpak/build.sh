#!/bin/sh
# Build the Forge Flatpak locally: `pnpm --dir app/ui flatpak [path/to/Forge_X.Y.Z_amd64.deb]`.
#
# Without an argument the manifest's release .deb is downloaded (its url and sha256 come from
# `pnpm --dir app/ui packaging:update vX.Y.Z`). With a local .deb (e.g. from
# `pnpm tauri build --bundles deb`) that file is used instead. The result is
# target/flatpak/Forge.flatpak; install it with `flatpak install --user target/flatpak/Forge.flatpak`.
#
# Needs flatpak-builder and the GNOME runtime and SDK of the manifest:
#   flatpak install --user flathub org.gnome.Platform//48 org.gnome.Sdk//48
# This script installs nothing on the system.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
manifest="$here/dev.forge.ide.yml"

if ! command -v flatpak-builder >/dev/null 2>&1; then
  echo "flatpak-builder is not installed: skipping the Flatpak build." >&2
  echo "Install it with your package manager (e.g. 'flatpak-builder' on Arch, Debian, Fedora)." >&2
  exit 0
fi

out="$root/target/flatpak"
work="${XDG_CACHE_HOME:-$HOME/.cache}/forge-flatpak"
mkdir -p "$out" "$work"

# A copy of the manifest with absolute paths, optionally using a local .deb.
build_manifest="$work/dev.forge.ide.yml"
sed "s|path: ../../|path: $root/|; s|path: dev.forge.ide|path: $here/dev.forge.ide|" "$manifest" > "$build_manifest"
if [ "$#" -gt 0 ]; then
  deb=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
  [ -f "$deb" ] || { echo "no such file: $1" >&2; exit 1; }
  # Replace the release download (url + sha256 + checker data) with the local file.
  awk -v deb="$deb" '
    /dest-filename: forge.deb/ { print; print "        path: " deb; skip = 1; next }
    skip && /^      - type:/ { skip = 0 }
    skip && /^        (url|sha256|only-arches):/ { next }
    skip && /^        x-checker-data:/ { inchecker = 1; next }
    inchecker && /^          / { next }
    { inchecker = 0; print }
  ' "$build_manifest" > "$build_manifest.tmp" && mv "$build_manifest.tmp" "$build_manifest"
fi

flatpak-builder --user --force-clean --state-dir="$work/state" \
  --repo="$work/repo" "$work/build" "$build_manifest"
flatpak build-bundle "$work/repo" "$out/Forge.flatpak" dev.forge.ide
echo "Built $out/Forge.flatpak"
