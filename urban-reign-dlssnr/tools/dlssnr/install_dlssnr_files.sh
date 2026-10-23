#!/usr/bin/env bash
# install_dlssnr_files.sh -- place the DLSS / DLSS-NR runtime files for Urban Reign (docs/DLSSNR.md).
#
# Modeled on the gow3_pc project's install_dlss_files.sh: it copies ONLY the user's own NVIDIA
# runtime DLLs from a folder they supply (for example the extracted DLSS-5 mod pack), and checks
# for the OptiScaler DLSS-NR fork's files. It never downloads anything.
#
# Usage (from an MSYS2 / Git-Bash shell on Windows, or any bash):
#   bash tools/dlssnr/install_dlssnr_files.sh --from <folder with nvngx_dlss.dll + nvngx_dlssnr.dll> [--to <game folder>]
#   bash tools/dlssnr/install_dlssnr_files.sh --check [--to <game folder>]
#
# <game folder> is the folder holding Urban Reign.exe / urban.exe (default: build/port/bin if it
# exists, else the current directory). The fork's files (dxgi.dll, OptiScaler.ini, the fork's
# nvngx.dll_dlssnr.dll forwarder) are copied by hand from the fork's release -- see docs/DLSSNR.md;
# --check reports which of them are present.
set -u

FROM=""
TO=""
CHECK=0
while [ $# -gt 0 ]; do
    case "$1" in
        --from) FROM="$2"; shift 2 ;;
        --to) TO="$2"; shift 2 ;;
        --check) CHECK=1; shift ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

if [ -z "$TO" ]; then
    if [ -f "build/port/bin/urban.exe" ]; then TO="build/port/bin";
    elif [ -f "build/port/bin/Urban Reign.exe" ]; then TO="build/port/bin";
    else TO="."; fi
fi

echo "game folder: $TO"

missing=0
note() { echo "  $1"; }
want() { # want <file> <what it is>
    if [ -f "$TO/$1" ]; then note "OK      $1 ($2)"; else note "MISSING $1 ($2)"; missing=1; fi
}

if [ "$CHECK" = "0" ]; then
    if [ -z "$FROM" ]; then echo "error: --from <folder> is required unless --check" >&2; exit 2; fi
    for f in nvngx_dlss.dll nvngx_dlssnr.dll; do
        # The mod pack nests them (dlss 5/, streamline_dlss/dlss/); accept a flat folder too.
        src="$(find "$FROM" -iname "$f" -print -quit 2>/dev/null)"
        if [ -z "$src" ]; then echo "error: $f not found under $FROM" >&2; exit 1; fi
        cp -f "$src" "$TO/$f" && echo "copied $f (from $src)"
    done
fi

echo "runtime files:"
want nvngx_dlss.dll "NVIDIA DLSS Super Resolution runtime -- the hook loads this"
want nvngx_dlssnr.dll "NVIDIA DLSS 5 Neural Rendering runtime -- the fork's NR pass loads this"
echo "OptiScaler DLSS-NR fork files (copy the fork's COMPLETE release here, docs/DLSSNR.md):"
want dxgi.dll "the fork's OptiScaler proxy DLL"
want OptiScaler.ini "the fork's settings (template: docs/dlssnr/OptiScaler.ini)"
want nvngx.dll_dlssnr.dll "the fork's NR forwarder (must be the fork's, with the _v2 exports)"

if [ "$missing" = "1" ]; then
    echo "some files are missing (see above). Without nvngx_dlss.dll the hook stays inactive;"
    echo "without the fork files you get plain DLSS DLAA instead of Neural Rendering."
    exit 1
fi
echo "all DLSS-NR files present."
