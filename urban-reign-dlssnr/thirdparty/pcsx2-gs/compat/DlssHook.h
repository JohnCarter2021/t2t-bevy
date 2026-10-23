// DLSS input hook for pcsx2gs.dll (docs/DLSSNR.md).
//
// Urban Reign makes no upscaler call of its own, so OptiScaler -- a middleware that intercepts
// DLSS/FSR/XeSS calls -- has nothing to intercept. This hook creates that call: on the DX12 GS
// device, at present time, it evaluates a genuine NVNGX DLSS Super Resolution feature (DLAA:
// render size == output size) on the composed frame, with the GS depth target replaced by a
// constant-depth texture and an all-zero motion-vector texture (the PS2 GS has neither a depth
// buffer for the display circuit nor motion vectors; see docs/DLSSNR.md for the limits this
// implies and the planned real-depth follow-up).
//
// With OptiScaler installed as dxgi.dll, its DLSS input layer receives this dispatch; the
// DLSS-NR fork (Dagherbou/OptiScaler_DLSSNR) then runs Neural Rendering as an extra pass inside
// it, using this dispatch's Color/Depth/MotionVectors. Without OptiScaler, the same dispatch is
// plain DLSS DLAA through the user's own nvngx_dlss.dll. Without that DLL (or on a non-NVIDIA
// GPU), the hook disables itself and presentation is unchanged.
//
// Every gate logs once to stderr with the [dlssnr] prefix; a failed evaluate disables the hook
// for the rest of the session instead of risking a bad frame loop.
#pragma once

#include "GS/GSVector.h"

class GSTexture;

namespace pcsx2gs::dlss
{
    // Set by the Bridge from the open options (dlssnr=1). When enabled, the Bridge also selects
    // the DX12 renderer; the hook itself no-ops on any other device.
    void SetEnabled(bool enabled);
    bool Enabled();

    // Called from GSRenderer::VSync, between BeginPresentFrame and PresentRect, on the GS
    // thread. Returns the DLSS output texture (sized exactly to src_rect) to present instead of
    // `current`, or nullptr to present `current` unchanged.
    GSTexture *ProcessFrame(GSTexture *current, const GSVector4i &src_rect);

    // Releases the DLSS feature and unloads NGX. Called by the Bridge before GSclose, while the
    // D3D12 device is still alive.
    void Shutdown();
} // namespace pcsx2gs::dlss
