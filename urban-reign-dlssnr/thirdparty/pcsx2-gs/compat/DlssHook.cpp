// DLSS input hook: see DlssHook.h and docs/DLSSNR.md.
//
// Runs entirely on the GS thread (the only thread that touches the pcsx2gs API). All NGX state
// lives in this file. The flow per presented frame:
//   1. Copy the composed display texture (src_rect region) into an owned color texture.
//   2. Clear an owned depth texture to 1.0 and an owned motion-vector texture to 0.
//   3. Evaluate the NGX DLSS feature (DLAA) into an owned UAV output texture.
//   4. Hand the output texture back; GSRenderer presents it with a full-range UV rect.
// Any failure along the way logs once and falls back to presenting the original texture; an
// evaluate failure disables the hook for the rest of the session.

#include "DlssHook.h"
#include "ngx_min.h"

#include "GS/Renderers/Common/GSDevice.h"
#include "GS/Renderers/DX12/GSDevice12.h"
#include "GS/Renderers/DX12/GSTexture12.h"

#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <string>

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

namespace
{
    bool g_enabled = false;
    bool g_disabled = false; // a gate failed; stay off until Shutdown/restart
    bool g_loggedInit = false;

    HMODULE g_ngxModule = nullptr;
    ngx::PFN_Init g_ngxInit = nullptr;
    ngx::PFN_Shutdown1 g_ngxShutdown = nullptr;
    ngx::PFN_GetParameters g_ngxGetParameters = nullptr;
    ngx::PFN_DestroyParameters g_ngxDestroyParameters = nullptr;
    ngx::PFN_CreateFeature g_ngxCreateFeature = nullptr;
    ngx::PFN_ReleaseFeature g_ngxReleaseFeature = nullptr;
    ngx::PFN_EvaluateFeature g_ngxEvaluateFeature = nullptr;

    ngx::Parameter *g_createParams = nullptr;
    ngx::Parameter *g_evalParams = nullptr;
    ngx::Handle *g_feature = nullptr;
    ID3D12Device *g_ngxDevice = nullptr;

    GSTexture *g_color = nullptr;  // owned copy of the display texture's active rect (RGBA8)
    GSTexture *g_depth = nullptr;  // owned depth, cleared to 1.0 every frame
    GSTexture *g_motion = nullptr; // owned motion vectors (RGBA16F), cleared to 0 every frame
    GSTexture *g_output = nullptr; // owned DLSS output (RGBA8, shader-write)
    int g_width = 0;
    int g_height = 0;
    const GSTexture *g_lastSource = nullptr;
    bool g_resetPending = true;
    unsigned long long g_evaluates = 0;

    void Log(const char *format, ...)
    {
        char buffer[512];
        va_list args;
        va_start(args, format);
        std::vsnprintf(buffer, sizeof(buffer), format, args);
        va_end(args);
        std::fprintf(stderr, "[dlssnr] %s\n", buffer);
        std::fflush(stderr);
    }

    void Disable(const char *reason, unsigned int result)
    {
        if (!g_disabled)
            Log("hook disabled: %s (NGX result 0x%08x)", reason, result);
        g_disabled = true;
    }

    void FreeTextures()
    {
        // All four work textures come from the device's pooled creators below; pooled textures
        // are released with Recycle, never deleted.
        if (g_gs_device)
        {
            if (g_color)
                g_gs_device->Recycle(g_color);
            if (g_depth)
                g_gs_device->Recycle(g_depth);
            if (g_motion)
                g_gs_device->Recycle(g_motion);
            if (g_output)
                g_gs_device->Recycle(g_output);
        }
        g_color = g_depth = g_motion = g_output = nullptr;
        g_width = g_height = 0;
    }

    void ReleaseFeature()
    {
        if (g_feature && g_ngxReleaseFeature)
            g_ngxReleaseFeature(g_feature);
        g_feature = nullptr;
    }

    // Loads nvngx_dlss.dll and resolves the D3D12 entry points. When OptiScaler is installed it
    // hooks these exports in the loaded module, so this same dispatch is what its DLSS input
    // layer (and the DLSS-NR fork's extra pass) receives.
    bool LoadNgx()
    {
        if (g_ngxModule)
            return true;
        g_ngxModule = LoadLibraryW(L"nvngx_dlss.dll");
        if (!g_ngxModule)
        {
            Log("nvngx_dlss.dll not found next to the exe; hook inactive (this is expected "
                "without the NVIDIA DLSS runtime installed)");
            return false;
        }
        g_ngxInit = reinterpret_cast<ngx::PFN_Init>(GetProcAddress(g_ngxModule, "NVSDK_NGX_D3D12_Init"));
        g_ngxShutdown = reinterpret_cast<ngx::PFN_Shutdown1>(GetProcAddress(g_ngxModule, "NVSDK_NGX_D3D12_Shutdown1"));
        g_ngxGetParameters = reinterpret_cast<ngx::PFN_GetParameters>(GetProcAddress(g_ngxModule, "NVSDK_NGX_D3D12_GetParameters"));
        g_ngxDestroyParameters = reinterpret_cast<ngx::PFN_DestroyParameters>(GetProcAddress(g_ngxModule, "NVSDK_NGX_D3D12_DestroyParameters"));
        g_ngxCreateFeature = reinterpret_cast<ngx::PFN_CreateFeature>(GetProcAddress(g_ngxModule, "NVSDK_NGX_D3D12_CreateFeature"));
        g_ngxReleaseFeature = reinterpret_cast<ngx::PFN_ReleaseFeature>(GetProcAddress(g_ngxModule, "NVSDK_NGX_D3D12_ReleaseFeature"));
        g_ngxEvaluateFeature = reinterpret_cast<ngx::PFN_EvaluateFeature>(GetProcAddress(g_ngxModule, "NVSDK_NGX_D3D12_EvaluateFeature"));
        if (!g_ngxInit || !g_ngxGetParameters || !g_ngxCreateFeature || !g_ngxReleaseFeature || !g_ngxEvaluateFeature)
        {
            Log("nvngx_dlss.dll is missing D3D12 entry points; hook inactive");
            return false;
        }
        Log("nvngx_dlss.dll loaded, D3D12 entry points resolved");
        return true;
    }

    bool InitNgx(ID3D12Device *device)
    {
        if (g_ngxDevice == device)
            return true;
        if (!LoadNgx())
            return false;

        wchar_t dataPath[MAX_PATH] = {};
        GetTempPathW(MAX_PATH, dataPath);
        std::wstring path(dataPath);
        path += L"UrbanReignNGX";
        CreateDirectoryW(path.c_str(), nullptr);

        // 0x5552424E = "URBN", an application id for the NGX log/data folder only.
        const ngx::Result result = g_ngxInit(0x5552424Eu, path.c_str(), device, nullptr, ngx::SdkVersionApi);
        if (!ngx::Succeeded(result))
        {
            Disable("NVSDK_NGX_D3D12_Init failed (non-NVIDIA GPU or unsupported driver?)", result);
            return false;
        }
        g_ngxDevice = device;
        Log("NGX initialized on the GS DX12 device");
        // The NGX driver worker finishes starting up asynchronously; creating a feature in the
        // same instant can fail spuriously (OptiScaler waits the same 500 ms after init).
        Sleep(500);
        return true;
    }

    bool EnsureTextures(GSDevice12 *device, int width, int height)
    {
        if (g_color && g_width == width && g_height == height)
            return true;
        FreeTextures();
        ReleaseFeature();
        GSDevice *base = device;
        g_color = base->CreateRenderTarget(width, height, GSTexture::Format::Color, false);
        g_output = base->CreateShaderWriteTarget(width, height, GSTexture::Format::Color, false);
        g_motion = base->CreateRenderTarget(width, height, GSTexture::Format::ColorHDR, false);
        g_depth = base->CreateDepthStencil(width, height, false);
        if (!g_color || !g_output || !g_motion || !g_depth)
        {
            Log("could not allocate the DLSS work textures at %dx%d; hook inactive", width, height);
            FreeTextures();
            return false;
        }
        g_width = width;
        g_height = height;
        g_resetPending = true;
        Log("work textures allocated at %dx%d (DLAA: render size == output size)", width, height);
        return true;
    }

    bool CreateFeature(ID3D12GraphicsCommandList *commandList)
    {
        if (g_feature)
            return true;
        if (!g_createParams)
        {
            const ngx::Result result = g_ngxGetParameters(&g_createParams);
            if (!ngx::Succeeded(result) || !g_createParams)
            {
                Disable("NVSDK_NGX_D3D12_GetParameters failed", result);
                return false;
            }
        }
        g_createParams->Reset();
        g_createParams->Set(ngx::ParamWidth, static_cast<unsigned int>(g_width));
        g_createParams->Set(ngx::ParamHeight, static_cast<unsigned int>(g_height));
        g_createParams->Set(ngx::ParamOutWidth, static_cast<unsigned int>(g_width));
        g_createParams->Set(ngx::ParamOutHeight, static_cast<unsigned int>(g_height));
        g_createParams->Set(ngx::ParamPerfQualityValue, ngx::PerfQualityDLAA);
        g_createParams->Set(ngx::ParamCreationNodeMask, 1u);
        g_createParams->Set(ngx::ParamVisibilityNodeMask, 1u);
        // MVLowRes: the motion texture is at render resolution. AutoExposure: the GS provides no
        // exposure texture, so DLSS must derive exposure itself. Not HDR: the display texture is
        // LDR RGBA8. Depth is not inverted (cleared to the far value 1.0).
        g_createParams->Set(ngx::ParamDLSSCreateFlags,
                            ngx::CreateFlagMVLowRes | ngx::CreateFlagAutoExposure);
        g_createParams->Set(ngx::ParamSharpness, 0.0f);

        const ngx::Result result =
            g_ngxCreateFeature(commandList, ngx::FeatureSuperSampling, g_createParams, &g_feature);
        if (!ngx::Succeeded(result) || !g_feature)
        {
            Disable("NVSDK_NGX_D3D12_CreateFeature (DLSS DLAA) failed", result);
            return false;
        }
        Log("DLSS feature created at %dx%d (DLAA)", g_width, g_height);
        g_resetPending = true;
        return true;
    }
} // namespace

namespace pcsx2gs::dlss
{
    void SetEnabled(bool enabled)
    {
        g_enabled = enabled;
        if (enabled && !g_loggedInit)
        {
            g_loggedInit = true;
            Log("hook enabled by the dlssnr option; the GS renderer is DX12 and this dispatch is "
                "the DLSS input OptiScaler intercepts");
        }
    }

    bool Enabled()
    {
        return g_enabled;
    }

    GSTexture *ProcessFrame(GSTexture *current, const GSVector4i &src_rect)
    {
        if (!g_enabled || g_disabled || !current)
            return nullptr;
        GSDevice *base = g_gs_device.get();
        if (!base || base->GetRenderAPI() != RenderAPI::D3D12)
            return nullptr;
        GSDevice12 *device = GSDevice12::GetInstance();

        const int width = src_rect.width();
        const int height = src_rect.height();
        if (width <= 0 || height <= 0)
            return nullptr;

        if (!InitNgx(device->GetDevice()))
            return nullptr;
        if (!EnsureTextures(device, width, height))
        {
            g_disabled = true;
            return nullptr;
        }

        const D3D12CommandList &commands = device->GetCommandList();
        ID3D12GraphicsCommandList *commandList = commands.list4.get();
        if (!commandList)
            return nullptr;
        if (!CreateFeature(commandList))
            return nullptr;

        // Stage the inputs. CopyRect/Clear* record onto the device's current command list and
        // leave each texture in its tracked state; the transitions below then put every NGX
        // input in a shader-readable state and the output in a UAV state, as NGX requires.
        base->CopyRect(current, g_color, src_rect, 0, 0);
        base->ClearDepth(g_depth, 1.0f);
        base->ClearRenderTarget(g_motion, 0u);

        GSTexture12 *color12 = static_cast<GSTexture12 *>(g_color);
        GSTexture12 *depth12 = static_cast<GSTexture12 *>(g_depth);
        GSTexture12 *motion12 = static_cast<GSTexture12 *>(g_motion);
        GSTexture12 *output12 = static_cast<GSTexture12 *>(g_output);
        color12->TransitionToState(commands, GSTexture12::ResourceState::ComputeShaderResource);
        depth12->TransitionToState(commands, GSTexture12::ResourceState::ComputeShaderResource);
        motion12->TransitionToState(commands, GSTexture12::ResourceState::ComputeShaderResource);
        output12->TransitionToState(commands, GSTexture12::ResourceState::ComputeShaderUAV);

        if (!g_evalParams)
        {
            const ngx::Result result = g_ngxGetParameters(&g_evalParams);
            if (!ngx::Succeeded(result) || !g_evalParams)
            {
                Disable("NVSDK_NGX_D3D12_GetParameters (evaluate) failed", result);
                return nullptr;
            }
        }
        if (current != g_lastSource)
        {
            g_lastSource = current;
            g_resetPending = true; // a new display target: do not accumulate across the switch
        }
        g_evalParams->Reset();
        g_evalParams->Set(ngx::ParamColor, color12->GetResource());
        g_evalParams->Set(ngx::ParamDepth, depth12->GetResource());
        g_evalParams->Set(ngx::ParamMotionVectors, motion12->GetResource());
        g_evalParams->Set(ngx::ParamOutput, output12->GetResource());
        g_evalParams->Set(ngx::ParamReset, g_resetPending ? 1 : 0);
        g_evalParams->Set(ngx::ParamJitterOffsetX, 0.0f);
        g_evalParams->Set(ngx::ParamJitterOffsetY, 0.0f);
        g_evalParams->Set(ngx::ParamMVScaleX, 1.0f);
        g_evalParams->Set(ngx::ParamMVScaleY, 1.0f);
        g_evalParams->Set(ngx::ParamDLSSPreExposure, 1.0f);
        g_evalParams->Set(ngx::ParamDLSSRenderSubrectW, static_cast<unsigned int>(width));
        g_evalParams->Set(ngx::ParamDLSSRenderSubrectH, static_cast<unsigned int>(height));
        g_evalParams->Set(ngx::ParamDLSSColorSubrectBaseX, 0u);
        g_evalParams->Set(ngx::ParamDLSSColorSubrectBaseY, 0u);
        g_evalParams->Set(ngx::ParamDLSSDepthSubrectBaseX, 0u);
        g_evalParams->Set(ngx::ParamDLSSDepthSubrectBaseY, 0u);
        g_evalParams->Set(ngx::ParamDLSSMVSubrectBaseX, 0u);
        g_evalParams->Set(ngx::ParamDLSSMVSubrectBaseY, 0u);
        g_evalParams->Set(ngx::ParamDLSSOutputSubrectBaseX, 0u);
        g_evalParams->Set(ngx::ParamDLSSOutputSubrectBaseY, 0u);

        const ngx::Result result = g_ngxEvaluateFeature(commandList, g_feature, g_evalParams, nullptr);
        if (!ngx::Succeeded(result))
        {
            Disable("NVSDK_NGX_D3D12_EvaluateFeature failed", result);
            return nullptr;
        }
        ++g_evaluates;
        if (g_evaluates <= 3 || (g_evaluates % 1800) == 0)
            Log("DLSS evaluate #%llu ok (%dx%d, reset=%d)", g_evaluates, width, height,
                g_resetPending ? 1 : 0);
        g_resetPending = false;

        // PresentRect samples its source as a pixel-shader resource.
        output12->TransitionToState(commands, GSTexture12::ResourceState::PixelShaderResource);
        return g_output;
    }

    void Shutdown()
    {
        ReleaseFeature();
        if (g_createParams && g_ngxDestroyParameters)
            g_ngxDestroyParameters(g_createParams);
        if (g_evalParams && g_ngxDestroyParameters)
            g_ngxDestroyParameters(g_evalParams);
        g_createParams = g_evalParams = nullptr;
        FreeTextures();
        if (g_ngxDevice && g_ngxShutdown)
            g_ngxShutdown(g_ngxDevice);
        g_ngxDevice = nullptr;
        if (g_ngxModule)
            FreeLibrary(g_ngxModule);
        g_ngxModule = nullptr;
        g_disabled = false;
        g_evaluates = 0;
        g_lastSource = nullptr;
        g_resetPending = true;
    }
} // namespace pcsx2gs::dlss
