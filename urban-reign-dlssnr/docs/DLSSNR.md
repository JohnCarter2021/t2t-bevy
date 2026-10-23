# DLSS Neural Rendering for Urban Reign (via OptiScaler)

Branch `feature/dlssnr-dx12`. Status: **v1 — built and ABI-verified, not yet compiled or run on
Windows.** The first VS2022 build and the first in-game log are the next step; every gate in the
hook logs to stderr with the `[dlssnr]` prefix so that log says exactly how far it got.

## Why it is built this way

OptiScaler is middleware: it intercepts a game's existing DLSS/FSR/XeSS calls and redirects
them to another backend. Urban Reign makes **no** upscaler call — the PCSX2 GS draws and
presents — so installing OptiScaler alone does nothing here. Neural Rendering lives only in
the DLSS-NR fork (Dagherbou/OptiScaler_DLSSNR and its neural-upstream descendants), which runs
NR as an extra pass *inside a DLSS Super Resolution dispatch*, consuming that dispatch's
Color, Depth and MotionVectors (with their subrect origins/sizes).

So this change builds the missing dispatch, natively, in the port:

1. **The hook** (`thirdparty/pcsx2-gs/compat/DlssHook.cpp`) — on the DX12 GS device, inside
   `GSRenderer::VSync` between `BeginPresentFrame` and `PresentRect`, it evaluates a genuine
   NVNGX DLSS feature (DLAA: render size == output size, so no resolution change) on the
   composed display texture, and the DLSS output is presented instead.
2. **The renderer switch** — the NGX D3D12 API needs a D3D12 device, and the fork's NR pass is
   a DX12 path, so `dlssnr=1` makes the Bridge open the GS with the DX12 renderer (the
   `GSDevice12` code was already in the tree; DX11 stays the default for everyone else).
3. **OptiScaler** — installed as `dxgi.dll`, its DLSS input layer receives the hook's dispatch
   exactly as if the game had native DLSS. Selecting the fork's DLSS-NR backend then runs
   Neural Rendering on it (pre-SR placement = at display-texture resolution here, since the
   dispatch is DLAA).

Without OptiScaler, the same build gives plain DLSS DLAA through your own `nvngx_dlss.dll`.
Without that DLL (or on a non-NVIDIA GPU), the hook disables itself and the game is unchanged.

## The honest limits of v1

The PS2 Graphics Synthesizer has no motion vectors, and the display circuit's output has no
depth buffer. The hook therefore feeds DLSS:

- **Color** — a copy of the composed display texture's active rect. Real.
- **Depth** — an owned texture cleared to the far value (1.0) every frame. *Placeholder.*
- **Motion vectors** — an owned texture cleared to 0 every frame. *Placeholder.*
- **Jitter** — 0, **Reset** — on the first frame, on size changes, and when the display
  target changes.

Consequences to expect in the first test: still and slow scenes should show the NR effect
properly; fast camera or fighter motion will smear/ghost where the missing motion data
matters, and NR's own motion handling gets no help from us. That is the same compromise the
ReShade-based NR injections make, and it is the known v2 task: capture the HW renderer's real
depth target (`GSTextureCache::Target` for the scene ZBUF) at draw time and hand it over,
then synthesize camera motion. Do not judge NR's ceiling by v1's motion quality.

Two more v1 notes: the re-present path (`GSRenderer::PresentCurrentFrame`, used when the game
is paused/re-shown) is not hooked, and the first DX12 run compiles PCSX2's DX12 shaders at
runtime (the shipped shader cache is DX11-only), so the first launch is slower.

## Build

Same as the port itself (Windows, VS2022, your `.iso` in `game/`):

```bat
python build.py
```

No new build options and no DLSS SDK are needed: the hook declares the NGX ABI itself
(`thirdparty/pcsx2-gs/compat/ngx_min.h`, declarations only — compare OptiScaler's own
`NVNGX_Parameter` reimplementation) and loads `nvngx_dlss.dll` dynamically.

## Install the runtime files

Next to `urban.exe`, in the folder the game runs from:

| File | What it is | Source |
| --- | --- | --- |
| `nvngx_dlss.dll` | NVIDIA DLSS SR runtime | your own copy (e.g. your DLSS-5 mod pack) |
| `nvngx_dlssnr.dll` | NVIDIA DLSS 5 NR runtime | your own copy (same pack) |
| `dxgi.dll` | the DLSS-NR fork's OptiScaler proxy | the fork's complete release |
| `OptiScaler.ini` | the fork's settings | template: `docs/dlssnr/OptiScaler.ini` (`Dx12Upscaler=dlss`) |
| `nvngx.dll_dlssnr.dll` | the fork's NR forwarder | the fork's release — **not** the gow project's forwarder and not the AMD-pack one; the fork's package verifier requires its `_v2` exports |

Helper (MSYS2 / Git-Bash), modeled on the gow project's installer — copies the two NVIDIA
runtimes from a folder you name and checks the whole set:

```bash
bash tools/dlssnr/install_dlssnr_files.sh --from "<extracted mod pack folder>"
bash tools/dlssnr/install_dlssnr_files.sh --check
```

The fork's own folder in the mod pack is `if OptiScaler neural-upstream` — install its
**complete** release, not just the proxy DLL (its docs are explicit about stale forwarders
being rejected).

## Turn it on

In the port's `config.ini`:

```ini
[Video]
DlssNr=1
```

(Equivalently `PS2X_PCSX2_DLSSNR=1` in the environment.) Then, in game, press **Insert** for
the fork's overlay and select the DLSS-NR backend with "Apply before Super Resolution";
NR tuning (Intensity, Style, Local Structure, skin controls…) lives in that overlay.

## What a good first log looks like

The game writes stderr to its console/log. In order:

```text
[pcsx2-gs] DLSS-NR hook on: DX12 renderer + DLSS DLAA input dispatch (docs/DLSSNR.md)
[dlssnr] hook enabled by the dlssnr option; ...
[dlssnr] nvngx_dlss.dll loaded, D3D12 entry points resolved
[dlssnr] NGX initialized on the GS DX12 device
[dlssnr] work textures allocated at WxH (DLAA: render size == output size)
[dlssnr] DLSS feature created at WxH (DLAA)
[dlssnr] DLSS evaluate #1 ok (WxH, reset=1)
[dlssnr] DLSS evaluate #2 ok ...
```

And in `OptiScaler.log`: the fork registering a DLSS input and creating its NR feature. If
the game log stops at any line, that line names the gate that failed — send the log. If the
hook's evaluate fails, it disables itself and the game keeps running unmodified.

## Files changed

- `thirdparty/pcsx2-gs/compat/DlssHook.{h,cpp}`, `ngx_min.h` — the hook and the NGX ABI.
- `thirdparty/pcsx2/pcsx2/GS/Renderers/Common/GSRenderer.cpp` — the present-path call (VSync).
- `thirdparty/pcsx2-gs/compat/Bridge.cpp` — `dlssnr=1` option, DX12 selection, hook lifecycle.
- `thirdparty/pcsx2-gs/include/pcsx2gs_api.h` — documents the new option key (ABI unchanged).
- `thirdparty/pcsx2-gs/CMakeLists.txt` — builds `DlssHook.cpp` into the GS library.
- `runtime/ps2xRuntime/src/lib/gs/gs_pcsx2_backend.cpp` — forwards `PS2X_PCSX2_DLSSNR`.
- `src/main.cpp` — `[Video] DlssNr` in `config.ini` maps to that variable.
- `docs/dlssnr/OptiScaler.ini`, `tools/dlssnr/install_dlssnr_files.sh` — install side.

## Verification done for v1

- The NGX ABI header compiles clean (`g++ -std=c++20 -Wall -Wextra`) and a fake-parameter
  smoke test proves the parameter vtable layout and overload routing (a D3D12 resource lands
  in the D3D12 slot, not the D3D11/`void*` ones) and the SDK constant values (DLAA = 5,
  SuperSampling = 1, SDK version = 0x15).
- MSVC portability audit: no system-header shadowing (`DlssHook.h`, `ngx_min.h` are unique
  names), no POSIX-only calls, `NOMINMAX` is defined by the PCSX2 target flags and re-guarded
  in the hook, all APIs used are the ones the surrounding PCSX2 code already uses.
- **Not** done: compiling the PCSX2 tree (Windows/MSVC-only — first build is on your PC, as
  with the gow project) and any in-game run. State/resource transitions around the NGX
  evaluate are the first thing to check if the first run misbehaves: the hook transitions its
  four work textures through `GSTexture12`'s tracked states, but NGX's own internal barriers
  are outside that tracker; a fallback to the unmodified present protects the picture if an
  evaluate ever returns an error.
