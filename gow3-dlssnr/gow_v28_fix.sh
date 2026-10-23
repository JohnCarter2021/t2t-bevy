#!/usr/bin/env bash
# gow_v28_fix.sh - one-shot v28 apply + verify for gow3_pc (display anchor in OnFrameStart; includes the v21 depth fix and the v20 merge fix).
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/FlameBreather/Downloads/gow_v28_fix.sh
set -u
TREE=/c/gow/Opencode
RZD="$TREE/gpu/shadps4/video_core/renderer_vulkan"
PKGLOG="$TREE/package_v28.log"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 locate the v28 files =="
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
CM=$(find_file vk_camera_motion_v28.cpp) || fail "vk_camera_motion_v28.cpp not found - download all five v28 files into Downloads"
DL=$(find_file vk_dlss_v28.cpp) || fail "vk_dlss_v28.cpp not found in Downloads"
CMH=$(find_file vk_camera_motion_v28.h) || fail "vk_camera_motion_v28.h not found in Downloads"
TUH=$(find_file vk_temporal_upscaler_v28.h) || fail "vk_temporal_upscaler_v28.h not found in Downloads"
TU=$(find_file vk_temporal_upscaler_v28.cpp) || fail "vk_temporal_upscaler_v28.cpp not found in Downloads"
BR=$(find_file gow3_dlss_bridge_v28.cpp) || fail "gow3_dlss_bridge_v28.cpp not found in Downloads"
echo "all files found"

echo "== 2/5 install into the tree =="
cp "$CM" "$RZD/vk_camera_motion.cpp" || fail "copy vk_camera_motion.cpp failed"
cp "$DL" "$RZD/vk_dlss.cpp" || fail "copy vk_dlss.cpp failed"
cp "$CMH" "$RZD/vk_camera_motion.h" || fail "copy vk_camera_motion.h failed"
cp "$TUH" "$RZD/vk_temporal_upscaler.h" || fail "copy vk_temporal_upscaler.h failed"
cp "$TU" "$RZD/vk_temporal_upscaler.cpp" || fail "copy vk_temporal_upscaler.cpp failed"
cp "$BR" "$TREE/gpu/dlss_bridge/gow3_dlss_bridge.cpp" || fail "copy gow3_dlss_bridge.cpp failed"
N=$(grep -c "v21 depth kept" "$RZD/vk_camera_motion.cpp")
echo "fingerprint 'v21 depth kept' in vk_camera_motion.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_camera_motion.cpp in the tree is not the v28 version"
N=$(grep -c "v26: D3D" "$RZD/vk_camera_motion.cpp")
echo "fingerprint 'v26: D3D' in vk_camera_motion.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_camera_motion.cpp in the tree is not the v26 D3D version"
N=$(grep -c "display_checked" "$RZD/vk_camera_motion.h")
echo "fingerprint 'display_checked' in vk_camera_motion.h: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_camera_motion.h in the tree is not the v28 version"
N=$(grep -c "v24 display anchor" "$RZD/vk_temporal_upscaler.cpp")
echo "fingerprint 'v24 display anchor' in vk_temporal_upscaler.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_temporal_upscaler.cpp in the tree is not the v20/v28 version"
N=$(grep -c "v27: DLSS in LDR mode" "$RZD/vk_temporal_upscaler.cpp")
echo "fingerprint 'v27: DLSS in LDR mode' in vk_temporal_upscaler.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_temporal_upscaler.cpp in the tree is not the v27 version"
N=$(grep -c "pixel MVs, v28" "$TREE/gpu/dlss_bridge/gow3_dlss_bridge.cpp")
echo "fingerprint 'pixel MVs, v28' in gow3_dlss_bridge.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "gow3_dlss_bridge.cpp in the tree is not the v28 version"

echo "== 3/5 build (this takes a while) =="
cd "$TREE" || fail "cannot cd into $TREE"
bash build.sh || fail "build.sh FAILED - the compiler error lines above are exactly what Lance needs"
echo "build ok"

echo "== rebuilding the DLSS bridge DLL (v28 MV scale) =="
DLSS_SDK_ROOT=/c/sdk/DLSS bash packaging/windows/build_dlss.sh || fail "build_dlss.sh FAILED - the error lines above are exactly what Lance needs"
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
    bash packaging/windows/package.sh >"$PKGLOG" 2>&1 || fail "package.sh FAILED twice - upload the file C:\\gow\\Opencode\\package_v28.log to Lance"
fi
tail -3 "$PKGLOG"
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "package ok"

echo "== 5/5 verify the packaged exe contains v28 =="
HIT=$(grep -rl "v24 display anchor" dist/gow3-windows/bin | head -1)
echo "v28 found in: $HIT"
[ -n "$HIT" ] || fail "v28 string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v28 is built, packaged, and verified."
echo "IMPORTANT: launch the game NORMALLY this time (double-click the exe,"
echo "do NOT use the cmd window with the GOW3_UPSCALE_BEFORE_CS variable)."
echo "Upscaler = DLSS, Neural Rendering UNTICKED. Load the Hades save,"
echo "fight a few minutes, quit, and send Lance the log."
