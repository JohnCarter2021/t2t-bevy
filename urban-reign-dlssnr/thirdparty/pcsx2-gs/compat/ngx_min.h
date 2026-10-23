// Minimal NVNGX (DLSS) D3D12 ABI declarations for DlssHook.cpp.
//
// These are ABI declarations only (function signatures, the parameter vtable layout, result
// codes and parameter-name strings), written for interoperability with NVIDIA's nvngx_dlss.dll
// at run time -- the same approach OptiScaler takes with its own NVNGX_Parameter implementation.
// No NVIDIA SDK header is included or copied; the DLL itself is loaded dynamically and every
// entry point is resolved with GetProcAddress, so this code builds and runs (as a no-op) on
// machines without an NVIDIA GPU or without the DLL present.
#pragma once

#include <cstdint>

struct ID3D11Resource;
struct ID3D12Device;
struct ID3D12Resource;
struct ID3D12GraphicsCommandList;

namespace ngx
{
    using Result = std::uint32_t;
    inline constexpr Result ResultSuccess = 0x00000001u;
    inline bool Succeeded(Result r) { return (r & 0xFFF00000u) != 0xBAD00000u; } // NVSDK_NGX_SUCCEED

    inline constexpr unsigned int FeatureSuperSampling = 1u; // NVSDK_NGX_Feature_SuperSampling
    inline constexpr unsigned int PerfQualityDLAA = 5u;      // NVSDK_NGX_PerfQuality_Value_DLAA
    inline constexpr unsigned int SdkVersionApi = 0x15u;     // NVSDK_NGX_Version_API (NGX 1.5.0)

    // DLSS feature-create flags (NVSDK_NGX_DLSS_Feature_Flags_*).
    inline constexpr int CreateFlagIsHDR = 1 << 0;
    inline constexpr int CreateFlagMVLowRes = 1 << 1;
    inline constexpr int CreateFlagDepthInverted = 1 << 3;
    inline constexpr int CreateFlagAutoExposure = 1 << 6;

    struct Handle
    {
        unsigned int id;
    };

    // NVSDK_NGX_Parameter: a pure-virtual interface. The vtable order below is the ABI --
    // Set overloads first (ULL, float, double, uint, int, D3D11Resource, D3D12Resource, void*),
    // then the matching Get overloads, then Reset. Instances are obtained from the DLL through
    // NVSDK_NGX_D3D12_GetParameters; we never implement or delete them.
    struct Parameter
    {
        virtual void Set(const char *name, unsigned long long value) = 0;
        virtual void Set(const char *name, float value) = 0;
        virtual void Set(const char *name, double value) = 0;
        virtual void Set(const char *name, unsigned int value) = 0;
        virtual void Set(const char *name, int value) = 0;
        virtual void Set(const char *name, ID3D11Resource *value) = 0; // unused here
        virtual void Set(const char *name, ID3D12Resource *value) = 0;
        virtual void Set(const char *name, void *value) = 0;

        virtual Result Get(const char *name, unsigned long long *value) const = 0;
        virtual Result Get(const char *name, float *value) const = 0;
        virtual Result Get(const char *name, double *value) const = 0;
        virtual Result Get(const char *name, unsigned int *value) const = 0;
        virtual Result Get(const char *name, int *value) const = 0;
        virtual Result Get(const char *name, ID3D11Resource **value) const = 0; // unused here
        virtual Result Get(const char *name, ID3D12Resource **value) const = 0;
        virtual Result Get(const char *name, void **value) const = 0;

        virtual void Reset() = 0;

    protected:
        ~Parameter() = default;
    };

    // Parameter names (NVSDK_NGX_Parameter_* string keys).
    inline constexpr const char *ParamWidth = "Width";
    inline constexpr const char *ParamHeight = "Height";
    inline constexpr const char *ParamOutWidth = "OutWidth";
    inline constexpr const char *ParamOutHeight = "OutHeight";
    inline constexpr const char *ParamPerfQualityValue = "PerfQualityValue";
    inline constexpr const char *ParamCreationNodeMask = "CreationNodeMask";
    inline constexpr const char *ParamVisibilityNodeMask = "VisibilityNodeMask";
    inline constexpr const char *ParamDLSSCreateFlags = "DLSS.Feature.Create.Flags";
    inline constexpr const char *ParamSharpness = "Sharpness";

    inline constexpr const char *ParamColor = "Color";
    inline constexpr const char *ParamDepth = "Depth";
    inline constexpr const char *ParamMotionVectors = "MotionVectors";
    inline constexpr const char *ParamOutput = "Output";
    inline constexpr const char *ParamReset = "Reset";
    inline constexpr const char *ParamJitterOffsetX = "Jitter.Offset.X";
    inline constexpr const char *ParamJitterOffsetY = "Jitter.Offset.Y";
    inline constexpr const char *ParamMVScaleX = "MV.Scale.X";
    inline constexpr const char *ParamMVScaleY = "MV.Scale.Y";
    inline constexpr const char *ParamDLSSPreExposure = "DLSS.Pre.Exposure";
    inline constexpr const char *ParamDLSSRenderSubrectW = "DLSS.Render.Subrect.Dimensions.Width";
    inline constexpr const char *ParamDLSSRenderSubrectH = "DLSS.Render.Subrect.Dimensions.Height";
    inline constexpr const char *ParamDLSSColorSubrectBaseX = "DLSS.Input.Color.Subrect.Base.X";
    inline constexpr const char *ParamDLSSColorSubrectBaseY = "DLSS.Input.Color.Subrect.Base.Y";
    inline constexpr const char *ParamDLSSDepthSubrectBaseX = "DLSS.Input.Depth.Subrect.Base.X";
    inline constexpr const char *ParamDLSSDepthSubrectBaseY = "DLSS.Input.Depth.Subrect.Base.Y";
    inline constexpr const char *ParamDLSSMVSubrectBaseX = "DLSS.Input.MV.Subrect.Base.X";
    inline constexpr const char *ParamDLSSMVSubrectBaseY = "DLSS.Input.MV.Subrect.Base.Y";
    inline constexpr const char *ParamDLSSOutputSubrectBaseX = "DLSS.Output.Subrect.Base.X";
    inline constexpr const char *ParamDLSSOutputSubrectBaseY = "DLSS.Output.Subrect.Base.Y";

    // Entry points resolved from nvngx_dlss.dll (NVSDK_NGX_D3D12_*).
    using PFN_Init = Result(__cdecl *)(unsigned long long applicationId, const wchar_t *applicationDataPath,
                                       ID3D12Device *device, const void *featureCommonInfo, unsigned int sdkVersion);
    using PFN_Shutdown1 = Result(__cdecl *)(ID3D12Device *device);
    using PFN_GetParameters = Result(__cdecl *)(Parameter **outParameters);
    using PFN_DestroyParameters = Result(__cdecl *)(Parameter *parameters);
    using PFN_CreateFeature = Result(__cdecl *)(ID3D12GraphicsCommandList *commandList, unsigned int featureId,
                                                Parameter *parameters, Handle **outHandle);
    using PFN_ReleaseFeature = Result(__cdecl *)(Handle *handle);
    using PFN_EvaluateFeature = Result(__cdecl *)(ID3D12GraphicsCommandList *commandList, const Handle *handle,
                                                  Parameter *parameters, void *progressCallback);
} // namespace ngx
