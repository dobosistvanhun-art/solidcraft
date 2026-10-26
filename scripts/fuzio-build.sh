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
# The first build takes 30–60 minutes; later ones only rebuild what changed. The build is made for
# the machine it runs on (see "Fordítás" below).
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
# Optimised for this machine: code for its own CPU (on the ZBook Haswell: AVX2, FMA), the mimalloc
# allocator and the `fuzio` profile (Cargo.toml). SOLIDCRAFT_CPU=x86-64-v2 builds a binary that
# also runs on older machines.
CPU="${SOLIDCRAFT_CPU:-native}"
echo "  CPU: $CPU ($(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2- | sed 's/^ *//'))"
JOBS="$(nproc 2>/dev/null || echo 4)"
# Thin LTO of the app needs about 1.5 GB per job; keep the build within the memory there is.
MEM_GB="$(awk '/MemAvailable/ {print int($2/1048576)}' /proc/meminfo 2>/dev/null || echo 8)"
[ "$MEM_GB" -lt 4 ] && MEM_GB=4
MAXJ=$(( MEM_GB * 2 / 3 ))
[ "$JOBS" -gt "$MAXJ" ] && JOBS="$MAXJ"
RUSTFLAGS="-C target-cpu=$CPU ${RUSTFLAGS:-}" nice cargo build --profile fuzio -j "$JOBS" -p solidcraft --features mimalloc

echo "==> Csomag"
rm -rf "$OUT/pkg"
mkdir -p "$OUT/pkg/solidcraft"
cp target/fuzio/solidcraft "$OUT/pkg/solidcraft/solidcraft"
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
