#!/usr/bin/env bash
# gow_v20_fix.sh - one-shot v20 apply + verify for gow3_pc (FP16 merge copy, reduced path).
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/FlameBreather/Downloads/gow_v20_fix.sh
set -u
TREE=/c/gow/Opencode
RZD="$TREE/gpu/shadps4/video_core/renderer_vulkan"
PKGLOG="$TREE/package_v20.log"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 locate the v20 files =="
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
DL=$(find_file vk_dlss_v20.cpp) || fail "vk_dlss_v20.cpp not found - download all three v20 files into Downloads"
TUH=$(find_file vk_temporal_upscaler_v20.h) || fail "vk_temporal_upscaler_v20.h not found in Downloads"
TU=$(find_file vk_temporal_upscaler_v20.cpp) || fail "vk_temporal_upscaler_v20.cpp not found in Downloads"
echo "all files found"

echo "== 2/5 install into the tree =="
cp "$DL" "$RZD/vk_dlss.cpp" || fail "copy vk_dlss.cpp failed"
cp "$TUH" "$RZD/vk_temporal_upscaler.h" || fail "copy vk_temporal_upscaler.h failed"
cp "$TU" "$RZD/vk_temporal_upscaler.cpp" || fail "copy vk_temporal_upscaler.cpp failed"
N=$(grep -c "merge_image" "$RZD/vk_temporal_upscaler.h")
echo "fingerprint 'merge_image' in header: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_temporal_upscaler.h in the tree is not the v20 version"
N=$(grep -c "v20 merge via" "$RZD/vk_temporal_upscaler.cpp")
echo "fingerprint 'v20 merge via' in vk_temporal_upscaler.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_temporal_upscaler.cpp in the tree is not the v20 version"

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
    bash packaging/windows/package.sh >"$PKGLOG" 2>&1 || fail "package.sh FAILED twice - upload the file C:\\gow\\Opencode\\package_v20.log to Lance"
fi
tail -3 "$PKGLOG"
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "package ok"

echo "== 5/5 verify the packaged exe contains v20 =="
HIT=$(grep -rl "v20 merge via" dist/gow3-windows/bin | head -1)
echo "v20 found in: $HIT"
[ -n "$HIT" ] || fail "v20 string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v20 is built, packaged, and verified."
echo "Test with Upscaler = DLSS and Neural Rendering UNTICKED, play a few minutes."
echo "Watch for the line: Upscaler: v20 merge via FP16 copy (guest format 43)"
echo "If it STILL freezes, send the log - the prints will show how far it got."
