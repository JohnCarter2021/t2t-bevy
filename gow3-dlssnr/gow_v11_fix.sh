#!/usr/bin/env bash
# gow_v11_fix.sh - one-shot v11 apply + verify for gow3_pc.
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/saifk/Downloads/gow_v11_fix.sh
# It installs vk_rasterizer_v11.cpp into the tree, verifies it, builds,
# packages, deploys the DLSS DLLs, and verifies the packaged exe really
# contains v11. On any failure it stops and names the stage that failed.
set -u
TREE=/c/gow/Opencode
TARGET="$TREE/gpu/shadps4/video_core/renderer_vulkan/vk_rasterizer.cpp"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 locate the v11 file =="
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CPP=""
for d in "$SCRIPT_DIR" /c/Users/FlameBreather/Downloads /c/gow /c/Users/FlameBreather/Desktop; do
    if [ -f "$d/vk_rasterizer_v11.cpp" ]; then
        CPP="$d/vk_rasterizer_v11.cpp"
        break
    fi
done
[ -n "$CPP" ] || fail "vk_rasterizer_v11.cpp not found in Downloads, C:\\gow, or Desktop - download it from the Drive link Lance sent, into Downloads"
echo "found: $CPP"

echo "== 2/5 install into the tree =="
cp "$CPP" "$TARGET" || fail "copy into the tree failed"
N=$(grep -c "constants pattern" "$TARGET")
echo "source fingerprint (want 1): $N"
[ "$N" = "1" ] || fail "the file in the tree is still not v11"

echo "== 3/5 build (this takes a while) =="
cd "$TREE" || fail "cannot cd into $TREE"
bash build.sh || fail "build.sh FAILED - the compiler error lines above are exactly what Lance needs"
echo "build ok"

echo "== 4/5 package and deploy DLLs =="
bash packaging/windows/package.sh || fail "package.sh FAILED - the error lines above are exactly what Lance needs"
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "package ok"

echo "== 5/5 verify the packaged exe contains v11 =="
HIT=$(grep -rl "constants pattern" dist/gow3-windows/bin | head -1)
echo "v11 found in: $HIT"
[ -n "$HIT" ] || fail "v11 string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v11 is built, packaged, and verified."
echo "Now play 3-4 minutes of real gameplay, quit, and check the log"
echo "contains the line 'constants scan:' before sending it to Lance."
