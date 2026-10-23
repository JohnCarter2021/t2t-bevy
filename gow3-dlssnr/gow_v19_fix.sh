#!/usr/bin/env bash
# gow_v19_fix.sh - one-shot v19 apply + verify for gow3_pc (DLSS feature in its own submission).
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/FlameBreather/Downloads/gow_v19_fix.sh
set -u
TREE=/c/gow/Opencode
RZD="$TREE/gpu/shadps4/video_core/renderer_vulkan"
PKGLOG="$TREE/package_v19.log"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 locate the v19 files =="
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
DL=$(find_file vk_dlss_v19.cpp) || fail "vk_dlss_v19.cpp not found - download both v19 files into Downloads"
TU=$(find_file vk_temporal_upscaler_v19.cpp) || fail "vk_temporal_upscaler_v19.cpp not found in Downloads"
echo "both files found"

echo "== 2/5 install into the tree =="
cp "$DL" "$RZD/vk_dlss.cpp" || fail "copy vk_dlss.cpp failed"
cp "$TU" "$RZD/vk_temporal_upscaler.cpp" || fail "copy vk_temporal_upscaler.cpp failed"
N=$(grep -c "evaluate #" "$RZD/vk_dlss.cpp")
echo "fingerprint 'evaluate #' in vk_dlss.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_dlss.cpp in the tree is not the v19 version"
N=$(grep -c "own submission (v19)" "$RZD/vk_temporal_upscaler.cpp")
echo "fingerprint 'own submission (v19)' in vk_temporal_upscaler.cpp: $N (want >= 1)"
[ "$N" -ge 1 ] || fail "vk_temporal_upscaler.cpp in the tree is not the v19 version"

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
    bash packaging/windows/package.sh >"$PKGLOG" 2>&1 || fail "package.sh FAILED twice - upload the file C:\\gow\\Opencode\\package_v19.log to Lance"
fi
tail -3 "$PKGLOG"
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "package ok"

echo "== 5/5 verify the packaged exe contains v19 =="
HIT=$(grep -rl "own submission" dist/gow3-windows/bin | head -1)
echo "v19 found in: $HIT"
[ -n "$HIT" ] || fail "v19 string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v19 is built, packaged, and verified."
echo "Test with Upscaler = DLSS and Neural Rendering UNTICKED."
echo "If it still freezes, send the log AND then try one no-rebuild test:"
echo "set Upscaler = FSR in the launcher (NR still off) and play a minute -"
echo "whether FSR runs or also freezes tells Lance which half to fix next."
