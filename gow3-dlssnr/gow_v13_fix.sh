#!/usr/bin/env bash
# gow_v13_fix.sh - v13 apply + verify for gow3_pc (cumulative: launcher fix +
# all diagnostics v9-v12 already in the tree; this script verifies them,
# builds, packages with a hardened packaging stage, deploys the DLSS DLLs,
# and verifies the packaged exe).
# Run inside the MSYS2 CLANG64 shell:  bash /c/Users/FlameBreather/Downloads/gow_v13_fix.sh
set -u
TREE=/c/gow/Opencode
TARGET="$TREE/gpu/shadps4/video_core/renderer_vulkan/vk_rasterizer.cpp"
PKGLOG="$TREE/package_v13.log"

fail() {
    echo
    echo "!!! STOPPED AT: $1"
    echo "Send Lance a screenshot or a copy of the output above."
    exit 1
}

echo "== 1/5 verify the tree already holds all modifications =="
[ -f "$TARGET" ] || fail "tree not found at $TREE - extract GOW-fixed-v13.zip into C:\\gow first, then rerun this script"
N=$(grep -c "v12 hit" "$TARGET")
M=$(grep -c "Diag12Scan" "$TARGET")
echo "source fingerprints (want 1 and 3+): v12-hit=$N diag12=$M"
if [ "$N" != "1" ] || [ "$M" -lt 3 ]; then
    fail "the tree source is not current - extract GOW-fixed-v13.zip into C:\\gow (Replace), then rerun this script"
fi

echo "== 2/5 build (this takes a while) =="
cd "$TREE" || fail "cannot cd into $TREE"
bash build.sh || fail "build.sh FAILED - the compiler error lines above are exactly what Lance needs"
echo "build ok"

echo "== 3/5 package (hardened: closes anything running from dist, logs everything, retries once) =="
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
    bash packaging/windows/package.sh >"$PKGLOG" 2>&1 || fail "package.sh FAILED twice - upload the file C:\\gow\\Opencode\\package_v13.log to Lance"
fi
tail -3 "$PKGLOG"
echo "package ok (full log: $PKGLOG)"

echo "== 4/5 deploy DLLs =="
cp out/gow3_dlss.dll out/nvngx.dll_dlssnr.dll out/nvngx_dlss.dll out/nvngx_dlssnr.dll dist/gow3-windows/bin/ || fail "copying the 4 DLLs into dist failed"
bash packaging/windows/install_dlss_files.sh --check --to dist/gow3-windows/bin || fail "the DLL check failed"
echo "DLLs ok"

echo "== 5/5 verify the packaged exe contains the current build =="
HIT=$(grep -rl "v12 hit" dist/gow3-windows/bin | head -1)
echo "current build found in: $HIT"
[ -n "$HIT" ] || fail "build string not found anywhere in dist/gow3-windows/bin - packaging did not pick up the new build"

echo
echo "ALL GOOD: v13 (all modifications) is built, packaged, and verified."
echo "Now play 3-4 minutes of real gameplay, quit, and check the log"
echo "contains the lines 'v12 scan:' and any 'v12 hit kind=' before sending it to Lance."
