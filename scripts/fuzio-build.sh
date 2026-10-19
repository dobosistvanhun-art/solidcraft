#!/bin/bash
# SolidCraft (István's fork, branch "fuzio") built and installed on Fúzió.
#
#   bash ~/Projektek/solidcraft/scripts/fuzio-build.sh
#
# On the Fúzió desktop it builds inside the development box (Ubuntu 24.04: the compiler and the
# libraries are installed there, the first time) and then installs the result with the Program
# Manager (Start menu entry; a later run updates it in place). Inside the box (Fejlesztő terminál)
# it only builds: target/fuzio/solidcraft-fuzio.tar.gz.
#
# The first build takes 30–60 minutes; later ones only rebuild what changed.
set -euo pipefail

SRC="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$SRC/target/fuzio"
PKG="$OUT/solidcraft-fuzio.tar.gz"

if [ "${1:-}" != "--in-box" ] && command -v fusion-dev >/dev/null 2>&1; then
    echo "==> Fordítás a fejlesztődobozban"
    (cd "$SRC" && fusion-dev box run bash "$SRC/scripts/fuzio-build.sh" --in-box)
    echo "==> Telepítés a Programkezelővel"
    fusion-programs install "$PKG" --name SolidCraft
    echo "Kész: Start menü → SolidCraft"
    exit 0
fi

as_root() { if [ "$(id -u)" = 0 ]; then "$@"; else sudo "$@"; fi; }

echo "==> Csomagok"
need=(build-essential pkg-config git curl ca-certificates libasound2-dev libxkbcommon-dev libwayland-dev libx11-dev
      libxrandr-dev libxi-dev libxcursor-dev libgl1-mesa-dev libfontconfig1-dev)
missing=()
for p in "${need[@]}"; do dpkg -s "$p" >/dev/null 2>&1 || missing+=("$p"); done
if [ ${#missing[@]} -gt 0 ]; then
    as_root apt-get update
    as_root apt-get install -y --no-install-recommends "${missing[@]}"
fi

echo "==> Rust"
if ! command -v cargo >/dev/null 2>&1; then
    [ -x "$HOME/.cargo/bin/cargo" ] || curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi
rustup update stable >/dev/null 2>&1 || true

echo "==> Forrás frissítése"
git -C "$SRC" pull --ff-only || echo "  (nem sikerült frissíteni — a meglévő forrásból fordít)"

echo "==> Fordítás (az első 30–60 perc)"
cd "$SRC"
nice cargo build --release -p solidcraft

echo "==> Csomag"
rm -rf "$OUT/pkg"
mkdir -p "$OUT/pkg/solidcraft"
cp target/release/solidcraft "$OUT/pkg/solidcraft/solidcraft"
strip "$OUT/pkg/solidcraft/solidcraft" || true
cp assets/icons/app/linux/exports/hicolor/256x256/apps/ai.storyteller.solidcraft.png "$OUT/pkg/solidcraft/solidcraft.png" 2>/dev/null \
    || cp assets/icons/app/linux/exports/hicolor/512x512/apps/ai.storyteller.solidcraft.png "$OUT/pkg/solidcraft/solidcraft.png"
cat > "$OUT/pkg/solidcraft/solidcraft.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=SolidCraft
Comment=Parametric 3D CAD (István's build)
Exec=solidcraft %f
Icon=solidcraft.png
Categories=Graphics;Engineering;
MimeType=application/x-solidcraft;model/step;model/stl;
EOF
tar -C "$OUT/pkg" -czf "$PKG" solidcraft
echo "Csomag: $PKG"
