#!/usr/bin/env bash
# gow_v29_fix.sh - one-shot v29 apply + verify for gow3_pc (bridge transfers via blit; includes all v20-v28 fixes already in the tree).
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/FlameBreather/Downloads/gow_v29_fix.sh
set -u
TREE=/c/gow/Opencode
RZD="$TREE/gpu/shadps4/video_core/renderer_vulkan"
PKGLOG="$TREE/package_v29.log"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 locate the v29 file =="
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
find_file() {
    for d in "$SCRIPT_DIR" /c/Users/FlameBreather/Downloads /c/gow /c/Users/FlameBreather/Desktop; do
        if [ -f "$d/$1" ]; then
            echo "$d/$1"
            return 0
        fi
    done
    return 1
}
TU=$(find_file vk_temporal_upscaler_v29.cpp) || fail "vk_temporal_upscaler_v29.cpp not found - download it into Downloads"
echo "file found"

echo "== 2/5 install into the tree =="
cp "$TU" "$RZD/vk_temporal_upscaler.cpp" || fail "copy vk_temporal_upscaler.cpp failed"
N=$(grep -c "v29 bridge transfer via blit" "$RZD/vk_temporal_upscaler.cpp")
echo "fingerprint 'v29 bridge transfer via blit' in vk_temporal_upscaler.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_temporal_upscaler.cpp in the tree is not the v29 version"
N=$(grep -c "v27: DLSS in LDR mode" "$RZD/vk_temporal_upscaler.cpp")
echo "fingerprint 'v27: DLSS in LDR mode' in vk_temporal_upscaler.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_temporal_upscaler.cpp in the tree is not the v27+ version"
N=$(grep -c "v26: D3D" "$RZD/vk_camera_motion.cpp")
echo "fingerprint 'v26: D3D' in vk_camera_motion.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_camera_motion.cpp in the tree is not the v26 D3D version - tell Lance"

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
    bash packaging/windows/package.sh >"$PKGLOG" 2>&1 || fail "package.sh FAILED twice - upload the file C:\\gow\\Opencode\\package_v29.log to Lance"
fi
tail -3 "$PKGLOG"
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "package ok"

echo "== 5/5 verify the packaged exe contains v29 =="
HIT=$(grep -rl "v29 bridge transfer via blit" dist/gow3-windows/bin | head -1)
echo "v29 found in: $HIT"
[ -n "$HIT" ] || fail "v29 string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v29 is built, packaged, and verified."
echo "Launch the game normally. Upscaler = DLSS, Neural Rendering UNTICKED."
echo "Load the Hades save, play a few minutes, quit, and send Lance the log"
echo "plus how the colors look."
