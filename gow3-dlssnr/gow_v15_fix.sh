#!/usr/bin/env bash
# gow_v15_fix.sh - one-shot v15 apply + verify for gow3_pc (the camera feed rewrite).
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/FlameBreather/Downloads/gow_v15_fix.sh
# Installs the 4 v15 files, verifies each, builds, packages (hardened), deploys the
# DLSS DLLs, and verifies the packaged exe contains the v15 feed.
set -u
TREE=/c/gow/Opencode
RZD="$TREE/gpu/shadps4/video_core/renderer_vulkan"
PKGLOG="$TREE/package_v15.log"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 locate the v15 files =="
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
CMH=$(find_file vk_camera_motion_v15.h) || fail "vk_camera_motion_v15.h not found - download all 4 v15 files into Downloads"
CMC=$(find_file vk_camera_motion_v15.cpp) || fail "vk_camera_motion_v15.cpp not found in Downloads"
RZ=$(find_file vk_rasterizer_v15.cpp) || fail "vk_rasterizer_v15.cpp not found in Downloads"
TU=$(find_file vk_temporal_upscaler_v15.cpp) || fail "vk_temporal_upscaler_v15.cpp not found in Downloads"
echo "all 4 files found"

echo "== 2/5 install into the tree =="
cp "$CMH" "$RZD/vk_camera_motion.h" || fail "copy vk_camera_motion.h failed"
cp "$CMC" "$RZD/vk_camera_motion.cpp" || fail "copy vk_camera_motion.cpp failed"
cp "$RZ" "$RZD/vk_rasterizer.cpp" || fail "copy vk_rasterizer.cpp failed"
cp "$TU" "$RZD/vk_temporal_upscaler.cpp" || fail "copy vk_temporal_upscaler.cpp failed"
check() {
    N=$(grep -c "$2" "$1")
    echo "fingerprint $2 in $(basename "$1"): $N (want >= $3)"
    [ "$N" -ge "$3" ] || fail "the file in the tree is not the v15 version: $(basename "$1")"
}
check "$RZD/vk_camera_motion.h" "OnViewProj" 1
check "$RZD/vk_camera_motion.cpp" "OnViewProj" 1
check "$RZD/vk_rasterizer.cpp" "OnViewProj" 1
check "$RZD/vk_temporal_upscaler.cpp" "hash_only" 1

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
    bash packaging/windows/package.sh >"$PKGLOG" 2>&1 || fail "package.sh FAILED twice - upload the file C:\\gow\\Opencode\\package_v15.log to Lance"
fi
tail -3 "$PKGLOG"
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "package ok"

echo "== 5/5 verify the packaged exe contains v15 =="
HIT=$(grep -rl "view-proj feed" dist/gow3-windows/bin | head -1)
echo "v15 found in: $HIT"
[ -n "$HIT" ] || fail "v15 string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v15 is built, packaged, and verified."
echo "Now play 3-4 minutes of real gameplay, moving the camera, then quit"
echo "and send Lance the full log. Watch for: 'view-proj feed active',"
echo "'v15 scene depth feed', and any 'Upscaler:' lines."
