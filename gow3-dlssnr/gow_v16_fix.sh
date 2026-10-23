#!/usr/bin/env bash
# gow_v16_fix.sh - one-shot v16 apply + verify for gow3_pc (the scene-color bridge).
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/FlameBreather/Downloads/gow_v16_fix.sh
# Installs the 2 v16 files, verifies each, builds, packages (hardened), deploys the
# DLSS DLLs, and verifies the packaged exe contains the v16 bridge.
set -u
TREE=/c/gow/Opencode
RZD="$TREE/gpu/shadps4/video_core/renderer_vulkan"
PKGLOG="$TREE/package_v16.log"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 locate the v16 files =="
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
TUH=$(find_file vk_temporal_upscaler_v16.h) || fail "vk_temporal_upscaler_v16.h not found - download both v16 files into Downloads"
TU=$(find_file vk_temporal_upscaler_v16.cpp) || fail "vk_temporal_upscaler_v16.cpp not found in Downloads"
echo "both files found"

echo "== 2/5 install into the tree =="
cp "$TUH" "$RZD/vk_temporal_upscaler.h" || fail "copy vk_temporal_upscaler.h failed"
cp "$TU" "$RZD/vk_temporal_upscaler.cpp" || fail "copy vk_temporal_upscaler.cpp failed"
N=$(grep -c "bridge_image" "$RZD/vk_temporal_upscaler.h")
echo "header fingerprint bridge_image: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "the header in the tree is not the v16 version"
N=$(grep -c "v16 scene bridge" "$RZD/vk_temporal_upscaler.cpp")
echo "source fingerprint 'v16 scene bridge': $N (want 1)"
[ "$N" = "1" ] || fail "the cpp in the tree is not the v16 version"

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
    bash packaging/windows/package.sh >"$PKGLOG" 2>&1 || fail "package.sh FAILED twice - upload the file C:\\gow\\Opencode\\package_v16.log to Lance"
fi
tail -3 "$PKGLOG"
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "package ok"

echo "== 5/5 verify the packaged exe contains v16 =="
HIT=$(grep -rl "v16 scene bridge" dist/gow3-windows/bin | head -1)
echo "v16 found in: $HIT"
[ -n "$HIT" ] || fail "v16 string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v16 is built, packaged, and verified."
echo "Now play 3-4 minutes of real gameplay, moving the camera, then quit"
echo "and send Lance the full log. Watch for: 'v16 scene bridge' and any"
echo "'DLSS:' lines after it - and note how the picture looks."
