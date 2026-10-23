// SPDX-FileCopyrightText: Copyright 2026 IFreemz (shadps4_dlss bridge), gow3 contributors
// SPDX-License-Identifier: MIT
//
// gow3_dlss.dll: the only part of the project that uses the NVIDIA DLSS (NGX) SDK. Built with
// MSVC (the SDK's libraries are MSVC only); the port loads it at run time.
// Adapted from IFreemz/shadPS4-Bloodborne-DLSS-FSR (dlss_bridge).
//
// The Neural Rendering (DLSS 5) runtime is reached through a second module, nvngx.dll_dlssnr.dll,
// because the runtime only accepts calls from a module whose path contains "nvngx.dll". That module
// is ours and is built from source next to this one; the runtime itself is NVIDIA's and the user
// supplies it.

#include <algorithm>
#include <array>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <string>

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include "gow3_dlss_bridge.h" // Vulkan first: the NGX headers expect its types
#include "gow3_dlssnr_forwarder.h"

#include <nvsdk_ngx_helpers.h>
#include <nvsdk_ngx_vk.h>
#include <nvsdk_ngx_helpers_vk.h>

namespace {

constexpr char ProjectId[] = "42359704-c9f3-4806-9fb5-d469000fdb8a";
constexpr char EngineVersion[] = "gow3-windows";

// The Neural Rendering runtime, and the forwarder that satisfies its caller check.
constexpr wchar_t NrRuntimeName[] = L"nvngx_dlssnr.dll";
constexpr wchar_t NrForwarderName[] = L"nvngx.dll_dlssnr.dll";
// Narrow copies for the log: Log() is a narrow snprintf, and a wide name printed with %s stops
// at the first character's null byte ("Neural Rendering: n does not export ...").
constexpr char NrRuntimeNameLog[] = "nvngx_dlssnr.dll";
constexpr char NrForwarderNameLog[] = "nvngx.dll_dlssnr.dll";

// The forwarder's entry points.
using FnProbe = int32_t (*)(const wchar_t*);
using FnInit = int32_t (*)(const wchar_t*, const wchar_t*, void*, void*, void*);
using FnCreate = void* (*)(const Gow3DlssNrCreateParams*);
using FnEvaluate = int32_t (*)(const Gow3DlssNrEvalParams*);
using FnRelease = void (*)(void*);
using FnError = const char* (*)();

// The runtime's own extension queries. These two are the only entry points it does not gate on
// the caller, so the bridge can ask it directly: the answers are the runtime's own, with the
// versions it expects.
using FnInstanceExtensions =
    NVSDK_NGX_Result(__cdecl*)(const NVSDK_NGX_FeatureDiscoveryInfo*, uint32_t*,
                                VkExtensionProperties**);
using FnDeviceExtensions =
    NVSDK_NGX_Result(__cdecl*)(VkInstance, VkPhysicalDevice, const NVSDK_NGX_FeatureDiscoveryInfo*,
                                uint32_t*, VkExtensionProperties**);

struct State {
    Gow3DlssLogFn log{};
    std::wstring dll_directory;
    std::wstring data_directory;
    const wchar_t* dll_path{};
    NVSDK_NGX_FeatureCommonInfo common{};
    NVSDK_NGX_FeatureDiscoveryInfo discovery{};
    NVSDK_NGX_Parameter* capabilities{};
    NVSDK_NGX_Parameter* parameters{};
    NVSDK_NGX_Handle* feature{};
    Gow3DlssFeature feature_desc{};
    VkDevice device{};
    bool configured{};
    bool initialized{};

    // Neural Rendering.
    HMODULE nr_forwarder{};
    HMODULE nr_runtime{};
    FnProbe nr_probe{};
    FnInit nr_init{};
    FnCreate nr_create{};
    FnEvaluate nr_evaluate{};
    FnRelease nr_release{};
    FnError nr_error{};
    FnInstanceExtensions nr_instance_extensions{};
    FnDeviceExtensions nr_device_extensions{};
    NVSDK_NGX_Parameter* nr_parameters{};
    NVSDK_NGX_Handle* nr_feature{};
    Gow3DlssNrFeature nr_feature_desc{};
    // The resource descriptors have to outlive the feature: the work the runtime records refers to
    // them, and the feature is released only once the GPU is done with it.
    NVSDK_NGX_Resource_VK nr_color{};
    NVSDK_NGX_Resource_VK nr_depth{};
    NVSDK_NGX_Resource_VK nr_motion{};
    NVSDK_NGX_Resource_VK nr_output{};
    std::wstring nr_runtime_path;
    bool nr_ready{};
    bool nr_initialized{};
};

State state;

template <typename... Args>
void Log(int warning, const char* format, Args... args) {
    if (!state.log)
        return;
    std::array<char, 1024> text{};
    std::snprintf(text.data(), text.size(), format, args...);
    state.log(warning, text.data());
}

bool Check(const char* operation, NVSDK_NGX_Result result) {
    if (NVSDK_NGX_FAILED(result)) {
        Log(1, "%s failed: 0x%08x", operation, static_cast<unsigned>(result));
        return false;
    }
    return true;
}

void NVSDK_CONV NgxLog(const char* message, NVSDK_NGX_Logging_Level, NVSDK_NGX_Feature) {
    Log(0, "NGX: %s", message ? message : "");
}

std::wstring Join(const std::wstring& directory, const wchar_t* name) {
    std::wstring path = directory;
    if (!path.empty() && path.back() != L'\\' && path.back() != L'/') {
        path += L'\\';
    }
    path += name;
    return path;
}

int32_t Configure(const wchar_t* dll_directory, const wchar_t* data_directory, Gow3DlssLogFn log) {
    state.log = log;
    state.dll_directory = dll_directory ? dll_directory : L".";
    state.data_directory = data_directory ? data_directory : L".";
    state.dll_path = state.dll_directory.c_str();
    state.common.PathListInfo = {&state.dll_path, 1};
    state.common.LoggingInfo = {NgxLog, NVSDK_NGX_LOGGING_LEVEL_OFF, false};
    state.discovery.SDKVersion = NVSDK_NGX_Version_API;
    state.discovery.FeatureID = NVSDK_NGX_Feature_SuperSampling;
    state.discovery.Identifier.IdentifierType = NVSDK_NGX_Application_Identifier_Type_Project_Id;
    state.discovery.Identifier.v.ProjectDesc = {ProjectId, NVSDK_NGX_ENGINE_TYPE_CUSTOM,
                                                EngineVersion};
    state.discovery.ApplicationDataPath = state.data_directory.c_str();
    state.discovery.FeatureInfo = &state.common;
    state.configured = true;
    return 1;
}

int32_t InstanceExtensions(uint32_t* count, const VkExtensionProperties** extensions) {
    VkExtensionProperties* required{};
    if (!state.configured ||
        !Check("Instance extension query",
               NVSDK_NGX_VULKAN_GetFeatureInstanceExtensionRequirements(&state.discovery, count,
                                                                       &required)))
        return 0;
    *extensions = required;
    return 1;
}

int32_t DeviceExtensions(VkInstance instance, VkPhysicalDevice physical, uint32_t* count,
                         const VkExtensionProperties** extensions) {
    if (!state.configured)
        return 0;
    NVSDK_NGX_FeatureRequirement support{};
    if (!Check("Feature requirement query",
               NVSDK_NGX_VULKAN_GetFeatureRequirements(instance, physical, &state.discovery,
                                                       &support)))
        return 0;
    if (support.FeatureSupported != NVSDK_NGX_FeatureSupportResult_Supported) {
        Log(1, "DLSS is not supported on this GPU or driver (code 0x%x)",
            static_cast<unsigned>(support.FeatureSupported));
        return 0;
    }
    VkExtensionProperties* required{};
    if (!Check("Device extension query",
               NVSDK_NGX_VULKAN_GetFeatureDeviceExtensionRequirements(
                   instance, physical, &state.discovery, count, &required)))
        return 0;
    *extensions = required;
    return 1;
}

int32_t Initialize(VkInstance instance, VkPhysicalDevice physical, VkDevice device,
                   PFN_vkGetInstanceProcAddr get_instance_proc,
                   PFN_vkGetDeviceProcAddr get_device_proc) {
    if (!state.configured || state.initialized)
        return 0;
    if (!Check("NGX initialization",
               NVSDK_NGX_VULKAN_Init_with_ProjectID(
                   ProjectId, NVSDK_NGX_ENGINE_TYPE_CUSTOM, EngineVersion,
                   state.data_directory.c_str(), instance, physical, device, get_instance_proc,
                   get_device_proc, &state.common)))
        return 0;
    state.initialized = true;
    state.device = device;
    if (!Check("Capability query", NVSDK_NGX_VULKAN_GetCapabilityParameters(&state.capabilities)))
        return 0;
    int available{}, needs_driver{};
    NVSDK_NGX_Parameter_GetI(state.capabilities, NVSDK_NGX_Parameter_SuperSampling_Available,
                             &available);
    NVSDK_NGX_Parameter_GetI(state.capabilities,
                             NVSDK_NGX_Parameter_SuperSampling_NeedsUpdatedDriver, &needs_driver);
    if (needs_driver) {
        Log(1, "DLSS needs a newer NVIDIA driver");
        return 0;
    }
    if (!available) {
        Log(1, "DLSS Super Resolution is not available on this system");
        return 0;
    }
    return 1;
}

void ReleaseFeature() {
    if (state.feature) {
        NVSDK_NGX_VULKAN_ReleaseFeature(state.feature);
        state.feature = nullptr;
    }
    if (state.parameters) {
        NVSDK_NGX_VULKAN_DestroyParameters(state.parameters);
        state.parameters = nullptr;
    }
}

int32_t CreateFeature(VkCommandBuffer command, const Gow3DlssFeature* desc) {
    if (!state.initialized || !state.capabilities || !desc)
        return 0;
    ReleaseFeature();
    if (!Check("Parameter allocation", NVSDK_NGX_VULKAN_AllocateParameters(&state.parameters)))
        return 0;
    for (const char* key : {NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_DLAA,
                            NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_Quality,
                            NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_Balanced,
                            NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_Performance,
                            NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_UltraPerformance,
                            NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_UltraQuality})
        NVSDK_NGX_Parameter_SetUI(state.parameters, key, desc->preset);
    static constexpr std::array qualities{
        NVSDK_NGX_PerfQuality_Value_DLAA, NVSDK_NGX_PerfQuality_Value_MaxQuality,
        NVSDK_NGX_PerfQuality_Value_Balanced, NVSDK_NGX_PerfQuality_Value_MaxPerf,
        NVSDK_NGX_PerfQuality_Value_UltraPerformance};
    NVSDK_NGX_DLSS_Create_Params settings{};
    settings.Feature.InWidth = desc->input_width;
    settings.Feature.InHeight = desc->input_height;
    settings.Feature.InTargetWidth = desc->output_width;
    settings.Feature.InTargetHeight = desc->output_height;
    settings.Feature.InPerfQualityValue = qualities[std::clamp(desc->quality, 0, 4)];
    settings.InFeatureCreateFlags =
        NVSDK_NGX_DLSS_Feature_Flags_MVLowRes |
        (desc->depth_inverted ? NVSDK_NGX_DLSS_Feature_Flags_DepthInverted : 0) |
        (desc->hdr ? NVSDK_NGX_DLSS_Feature_Flags_IsHDR | NVSDK_NGX_DLSS_Feature_Flags_AutoExposure
                   : 0);
    const auto result = NGX_VULKAN_CREATE_DLSS_EXT1(state.device, command, 1, 1, &state.feature,
                                                   state.parameters, &settings);
    if (!Check("DLSS feature creation", result) || !state.feature) {
        state.feature = nullptr;
        return 0;
    }
    state.feature_desc = *desc;
    Log(0, "DLSS %ux%u -> %ux%u, quality %d, preset %u%s", desc->input_width, desc->input_height,
        desc->output_width, desc->output_height, desc->quality, desc->preset,
        desc->hdr ? ", HDR input" : "");
    return 1;
}

NVSDK_NGX_Resource_VK Wrap(const Gow3DlssImage& image, bool writable) {
    return NVSDK_NGX_Create_ImageView_Resource_VK(image.view, image.image, image.range,
                                                  image.format, image.width, image.height,
                                                  writable);
}

int32_t Evaluate(VkCommandBuffer command, const Gow3DlssEvaluate* evaluate) {
    if (!state.feature || !state.parameters || !evaluate)
        return 0;
    auto color = Wrap(evaluate->color, false);
    auto depth = Wrap(evaluate->depth, false);
    auto motion = Wrap(evaluate->motion, false);
    auto output = Wrap(evaluate->output, true);
    NVSDK_NGX_VK_DLSS_Eval_Params parameters{};
    parameters.Feature.pInColor = &color;
    parameters.Feature.pInOutput = &output;
    parameters.Feature.InSharpness = evaluate->sharpness;
    parameters.pInDepth = &depth;
    parameters.pInMotionVectors = &motion;
    parameters.InRenderSubrectDimensions = {state.feature_desc.input_width,
                                            state.feature_desc.input_height};
    parameters.InJitterOffsetX = evaluate->jitter_x;
    parameters.InJitterOffsetY = evaluate->jitter_y;
    parameters.InReset = evaluate->reset;
    // v28: the port's motion vectors are in PIXELS (camera_motion.comp:
    // 'pixels, previous - current'; FSR's portable layer is calibrated for
    // that). NGX expects them normalized, so the scale is 1/render size.
    // At 1.0 NGX read a 5-pixel motion as 5 full screens and smeared its
    // history across the frame - the DLSS-only blue wash (FSR unaffected).
    parameters.InMVScaleX = 1.0f / float(state.feature_desc.input_width);
    parameters.InMVScaleY = 1.0f / float(state.feature_desc.input_height);
    static bool mv_scale_logged = false;
    if (!mv_scale_logged) {
        mv_scale_logged = true;
        Log(0, "evaluate MV scale %.6f %.6f (pixel MVs, v28)", parameters.InMVScaleX,
            parameters.InMVScaleY);
    }
    parameters.InPreExposure = parameters.InExposureScale = 1.0f;
    parameters.InFrameTimeDeltaInMsec = evaluate->frame_ms;
    return Check("DLSS evaluation", NGX_VULKAN_EVALUATE_DLSS_EXT(command, state.feature,
                                                                state.parameters, &parameters))
               ? 1
               : 0;
}

// ---------------------------------------------------------------------------------------------
// Neural Rendering.
// ---------------------------------------------------------------------------------------------

// Loads the forwarder, once. It is the module the runtime accepts calls from, so the bridge never
// calls the runtime itself.
bool LoadForwarder() {
    if (state.nr_forwarder)
        return state.nr_create != nullptr && state.nr_evaluate != nullptr;
    const auto path = Join(state.dll_directory, NrForwarderName);
    state.nr_forwarder = LoadLibraryW(path.c_str());
    if (!state.nr_forwarder) {
        Log(1, "Neural Rendering: %s is missing", NrForwarderNameLog);
        return false;
    }
    state.nr_probe = reinterpret_cast<FnProbe>(GetProcAddress(state.nr_forwarder, "Gow3DlssNrProbe"));
    state.nr_init = reinterpret_cast<FnInit>(GetProcAddress(state.nr_forwarder, "Gow3DlssNrInit"));
    state.nr_create =
        reinterpret_cast<FnCreate>(GetProcAddress(state.nr_forwarder, "Gow3DlssNrCreate"));
    state.nr_evaluate =
        reinterpret_cast<FnEvaluate>(GetProcAddress(state.nr_forwarder, "Gow3DlssNrEvaluate"));
    state.nr_release =
        reinterpret_cast<FnRelease>(GetProcAddress(state.nr_forwarder, "Gow3DlssNrRelease"));
    state.nr_error = reinterpret_cast<FnError>(GetProcAddress(state.nr_forwarder, "Gow3DlssNrError"));
    if (!state.nr_probe || !state.nr_init || !state.nr_create || !state.nr_evaluate ||
        !state.nr_release) {
        Log(1, "Neural Rendering: %s does not export the expected entry points", NrForwarderNameLog);
        return false;
    }
    return true;
}

// Loads the runtime itself, once. The bridge needs it for the two extension queries, which are
// the only entry points the runtime does not gate on the caller. Loading it here does not make
// the bridge a caller it accepts: the calls that touch the device go through the forwarder.
bool LoadRuntime() {
    if (state.nr_runtime)
        return state.nr_device_extensions != nullptr && state.nr_instance_extensions != nullptr;
    state.nr_runtime = LoadLibraryExW(state.nr_runtime_path.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
    if (!state.nr_runtime) {
        return false;
    }
    state.nr_instance_extensions = reinterpret_cast<FnInstanceExtensions>(
        GetProcAddress(state.nr_runtime, "NVSDK_NGX_VULKAN_GetFeatureInstanceExtensionRequirements"));
    state.nr_device_extensions = reinterpret_cast<FnDeviceExtensions>(
        GetProcAddress(state.nr_runtime, "NVSDK_NGX_VULKAN_GetFeatureDeviceExtensionRequirements"));
    return state.nr_device_extensions != nullptr && state.nr_instance_extensions != nullptr;
}

int32_t NrProbe(uint32_t* entries) {
    if (entries) {
        *entries = 0;
    }
    if (!state.configured)
        return 0;
    if (!LoadForwarder())
        return 0;
    state.nr_runtime_path = Join(state.dll_directory, NrRuntimeName);
    if (!std::filesystem::is_regular_file(state.nr_runtime_path)) {
        Log(1, "Neural Rendering: %s is missing", NrRuntimeNameLog);
        return 0;
    }
    const int32_t bits = state.nr_probe(state.nr_runtime_path.c_str());
    if (entries) {
        *entries = static_cast<uint32_t>(bits);
    }
    if (bits != (GOW3_DLSSNR_ENTRY_INIT | GOW3_DLSSNR_ENTRY_CREATE | GOW3_DLSSNR_ENTRY_EVALUATE |
                 GOW3_DLSSNR_ENTRY_RELEASE)) {
        Log(1, "Neural Rendering: the runtime does not expose a complete Vulkan surface (%d)", bits);
        return 0;
    }
    state.nr_ready = true;
    Log(0, "Neural Rendering: runtime found");
    return 1;
}

// The runtime's own answers to what it needs. It ignores the discovery information here, so
// there is no feature id to get wrong: the list is the same whatever is asked about.
int32_t NrInstanceExtensions(uint32_t* count, const VkExtensionProperties** extensions) {
    if (!state.nr_ready || !LoadRuntime())
        return 0;
    uint32_t total{};
    VkExtensionProperties* properties{};
    if (!Check("Neural Rendering instance extension query",
               state.nr_instance_extensions(&state.discovery, &total, &properties)) ||
        !properties) {
        return 0;
    }
    *count = total;
    *extensions = properties;
    return 1;
}

int32_t NrDeviceExtensions(VkInstance instance, VkPhysicalDevice physical, uint32_t* count,
                           const VkExtensionProperties** extensions) {
    if (!state.nr_ready || !LoadRuntime())
        return 0;
    uint32_t total{};
    VkExtensionProperties* properties{};
    if (!Check("Neural Rendering device extension query",
               state.nr_device_extensions(instance, physical, &state.discovery, &total, &properties)) ||
        !properties) {
        return 0;
    }
    *count = total;
    *extensions = properties;
    return 1;
}

int32_t NrInitialize(VkInstance instance, VkPhysicalDevice physical, VkDevice device) {
    if (!state.nr_ready || state.nr_initialized)
        return state.nr_initialized ? 1 : 0;
    if (!state.nr_init) {
        return 0;
    }
    if (!state.nr_init(state.nr_runtime_path.c_str(), state.data_directory.c_str(),
                       reinterpret_cast<void*>(instance), reinterpret_cast<void*>(physical),
                       reinterpret_cast<void*>(device))) {
        const char* detail = state.nr_error ? state.nr_error() : nullptr;
        Log(1, "Neural Rendering: %s",
            detail && *detail ? detail : "the runtime would not initialise on this device");
        return 0;
    }
    state.nr_initialized = true;
    // The parameter block comes from the NGX core, which is up by now: the bridge initialised it
    // for Super Resolution. The model reads its tuning from this block when the feature is built.
    if (!Check("Neural Rendering parameter allocation",
               NVSDK_NGX_VULKAN_AllocateParameters(&state.nr_parameters)) ||
        !state.nr_parameters) {
        state.nr_initialized = false;
        return 0;
    }
    Log(0, "Neural Rendering: ready");
    return 1;
}

void NrReleaseFeature() {
    if (state.nr_feature) {
        state.nr_release(state.nr_feature);
        state.nr_feature = nullptr;
    }
}

int32_t NrCreateFeature(VkCommandBuffer command, const Gow3DlssNrFeature* desc) {
    if (!state.nr_initialized || !state.nr_parameters || !desc)
        return 0;
    NrReleaseFeature();
    Gow3DlssNrCreateParams create{};
    create.command_buffer = command;
    create.params = state.nr_parameters;
    create.width = desc->input_width;
    create.height = desc->input_height;
    create.preset = desc->preset;
    create.intensity = desc->intensity;
    create.style = desc->style;
    create.local_structure = desc->local_structure;
    create.local_tone = desc->local_tone;
    create.skin_structure = desc->skin_structure;
    create.auto_mask = desc->auto_mask;
    create.ui_correction = desc->ui_correction;
    state.nr_feature = static_cast<NVSDK_NGX_Handle*>(state.nr_create(&create));
    if (!state.nr_feature) {
        const char* detail = state.nr_error ? state.nr_error() : nullptr;
        Log(1, "Neural Rendering: %s",
            detail && *detail ? detail : "the feature could not be created");
        return 0;
    }
    state.nr_feature_desc = *desc;
    Log(0, "Neural Rendering: feature %ux%u created", desc->input_width, desc->input_height);
    return 1;
}

int32_t NrEvaluate(VkCommandBuffer command, const Gow3DlssNrFrame* evaluate) {
    if (!state.nr_feature || !state.nr_parameters || !evaluate)
        return 0;
    // The descriptors are kept in the state rather than on the stack: the work the runtime records
    // refers to them, so they have to outlive the call and the feature.
    state.nr_color = Wrap(evaluate->color, false);
    state.nr_depth = Wrap(evaluate->depth, false);
    state.nr_motion = Wrap(evaluate->motion, false);
    state.nr_output = Wrap(evaluate->output, true);
    Gow3DlssNrEvalParams parameters{};
    parameters.command_buffer = command;
    parameters.feature = state.nr_feature;
    parameters.params = state.nr_parameters;
    parameters.color = &state.nr_color;
    parameters.depth = &state.nr_depth;
    parameters.motion = &state.nr_motion;
    parameters.output = &state.nr_output;
    parameters.width = state.nr_feature_desc.input_width;
    parameters.height = state.nr_feature_desc.input_height;
    parameters.guide_width = evaluate->guide_width;
    parameters.guide_height = evaluate->guide_height;
    parameters.motion_width = evaluate->motion_width;
    parameters.motion_height = evaluate->motion_height;
    parameters.depth_base_x = evaluate->depth_base_x;
    parameters.depth_base_y = evaluate->depth_base_y;
    parameters.motion_base_x = evaluate->motion_base_x;
    parameters.motion_base_y = evaluate->motion_base_y;
    parameters.depth_inverted = evaluate->depth_inverted;
    parameters.reset = evaluate->reset;
    parameters.intensity = evaluate->intensity;
    parameters.style = evaluate->style;
    parameters.local_structure = evaluate->local_structure;
    parameters.local_tone = evaluate->local_tone;
    parameters.skin_structure = evaluate->skin_structure;
    parameters.auto_mask = evaluate->auto_mask;
    parameters.mv_scale_x = evaluate->mv_scale_x;
    parameters.mv_scale_y = evaluate->mv_scale_y;
    return state.nr_evaluate(&parameters);
}

void Shutdown() {
    ReleaseFeature();
    NrReleaseFeature();
    if (state.nr_parameters) {
        NVSDK_NGX_VULKAN_DestroyParameters(state.nr_parameters);
        state.nr_parameters = nullptr;
    }
    if (state.capabilities) {
        NVSDK_NGX_VULKAN_DestroyParameters(state.capabilities);
        state.capabilities = nullptr;
    }
    if (state.initialized) {
        NVSDK_NGX_VULKAN_Shutdown1(state.device);
        state.initialized = false;
        state.device = VK_NULL_HANDLE;
    }
}

const Gow3DlssApi api{GOW3_DLSS_BRIDGE_ABI, Configure,      InstanceExtensions, DeviceExtensions,
                     Initialize,             CreateFeature,  Evaluate,           ReleaseFeature,
                     Shutdown,               NrProbe,        NrInstanceExtensions,
                     NrDeviceExtensions,     NrInitialize,   NrCreateFeature,    NrEvaluate,
                     NrReleaseFeature};

} // namespace

extern "C" __declspec(dllexport) const Gow3DlssApi* Gow3DlssGetApi() {
    return &api;
}
