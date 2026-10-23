#!/usr/bin/env bash
# gow_v14_fix.sh - one-shot v14 apply + verify for gow3_pc.
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/FlameBreather/Downloads/gow_v14_fix.sh
# Installs vk_rasterizer_v14.cpp into the tree, verifies it, builds, packages
# (hardened: closes anything running from dist first, logs everything, retries
# once), deploys the DLSS DLLs, and verifies the packaged exe contains v14.
set -u
TREE=/c/gow/Opencode
TARGET="$TREE/gpu/shadps4/video_core/renderer_vulkan/vk_rasterizer.cpp"
PKGLOG="$TREE/package_v14.log"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 locate the v14 file =="
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CPP=""
for d in "$SCRIPT_DIR" /c/Users/FlameBreather/Downloads /c/gow /c/Users/FlameBreather/Desktop; do
    if [ -f "$d/vk_rasterizer_v14.cpp" ]; then
        CPP="$d/vk_rasterizer_v14.cpp"
        break
    fi
done
[ -n "$CPP" ] || fail "vk_rasterizer_v14.cpp not found next to this script or in Downloads - download it from the Drive link Lance sent, into Downloads"
echo "found: $CPP"

echo "== 2/5 install into the tree =="
cp "$CPP" "$TARGET" || fail "copy into the tree failed"
N=$(grep -c "v14 dump size=" "$TARGET")
echo "source fingerprint (want 1): $N"
[ "$N" = "1" ] || fail "the file in the tree is still not v14"

echo "== 3/5 build (this takes a while) =="
cd "$TREE" || fail "cannot cd into $TREE"
bash build.sh || fail "build.sh FAILED - the compiler error lines above are exactly what Lance needs"
echo "build ok"

echo "== 4/5 package (hardened) and deploy DLLs =="
close_dist_processes() {
    powershell -NoProfile -Command "Get-Process | Where-Object { \$_.Path -like 'C:\gow\Opencode\dist\gow3-windows\*' } | Stop-Process -Force -ErrorAction SilentlyContinue" >/dev/null 2>&1 || true
}
close_dist_processes
sleep 1
if ! bash packaging/windows/package.sh >"$PKGLOG" 2>&1; then
    echo "first packaging attempt failed; last log lines:"
    tail -25 "$PKGLOG"
    echo "--- closing dist processes and retrying once ---"
    close_dist_processes
    sleep 2
    bash packaging/windows/package.sh >"$PKGLOG" 2>&1 || fail "package.sh FAILED twice - upload the file C:\\gow\\Opencode\\package_v14.log to Lance"
fi
tail -3 "$PKGLOG"
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "package ok"

echo "== 5/5 verify the packaged exe contains v14 =="
HIT=$(grep -rl "v14 dump size=" dist/gow3-windows/bin | head -1)
echo "v14 found in: $HIT"
[ -n "$HIT" ] || fail "v14 string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v14 is built, packaged, and verified."
echo "Now play 3-4 minutes of real gameplay - move the camera around -"
echo "quit, and send Lance the full log. The 'v14 dump' lines are the payload."
