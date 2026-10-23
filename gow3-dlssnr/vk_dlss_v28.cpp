// SPDX-FileCopyrightText: Copyright 2026 IFreemz, gow3 contributors
// SPDX-License-Identifier: GPL-2.0-or-later

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <mutex>
#include <optional>
#include <string_view>

#include "video_core/renderer_vulkan/vk_dlss.h"

#include "gow3_settings.h"

#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include "../../../dlss_bridge/gow3_dlss_bridge.h"
#endif

namespace Vulkan {

int Dlss::QualityForScale(float scale) {
    // DLAA only for equal sizes: any real upscale needs at least the Quality mode.
    return scale >= 2.9f ? 4 : scale >= 1.95f ? 3 : scale >= 1.65f ? 2 : scale > 1.01f ? 1 : 0;
}

#ifdef _WIN32

namespace {
constexpr wchar_t BridgeName[] = L"gow3_dlss.dll";
constexpr wchar_t NgxName[] = L"nvngx_dlss.dll";

std::filesystem::path ExecutableDirectory() {
    std::wstring path(MAX_PATH, L'\0');
    const DWORD length = GetModuleFileNameW(nullptr, path.data(), DWORD(path.size()));
    path.resize(length);
    return std::filesystem::path{path}.parent_path();
}

void BridgeLog(int warning, const char* message) {
    std::printf("DLSS: %s%s\n", warning ? "warning: " : "", message);
}

bool CollectExtensions(const VkExtensionProperties* required, u32 count,
                       const std::vector<vk::ExtensionProperties>& available,
                       std::vector<std::string>& storage, std::vector<const char*>& enabled) {
    storage.clear();
    for (u32 i = 0; i < count; ++i) {
        const auto& request = required[i];
        const auto match = std::ranges::find_if(available, [&](const auto& extension) {
            return std::strcmp(extension.extensionName.data(), request.extensionName) == 0 &&
                   extension.specVersion >= request.specVersion;
        });
        if (match == available.end()) {
            std::printf("DLSS: missing Vulkan extension %s\n", request.extensionName);
            return false;
        }
        storage.emplace_back(request.extensionName);
    }
    for (const auto& name : storage) {
        if (std::ranges::none_of(enabled, [&](const char* current) { return name == current; })) {
            enabled.push_back(name.c_str());
        }
    }
    return true;
}
} // namespace

struct Dlss::Impl {
    HMODULE module{};
    const Gow3DlssApi* api{};
    std::vector<std::string> instance_extensions, device_extensions;
    // The Neural Rendering extensions have to outlive the call that adds them: the device keeps
    // the pointers, not the names.
    std::vector<std::string> nr_instance_extensions;
    std::vector<std::string> nr_device_extensions;
    std::optional<FeatureDesc> feature;
    std::string problem;
    bool eligible{true}, instance_ready{}, device_ready{}, initialized{}, available{};

    // Neural Rendering.
    std::optional<NrFeatureDesc> nr_feature;
    std::string nr_problem;
    u32 nr_entries{};
    bool nr_ready{}, nr_initialized{}, nr_available{};

    void Disable(std::string_view reason) {
        eligible = available = false;
        problem = reason;
        std::printf("DLSS: unavailable (%.*s)\n", int(reason.size()), reason.data());
    }

    void DisableNr(std::string_view reason) {
        // The model is gone for this session: nothing else may bring it up afterwards.
        nr_ready = false;
        nr_available = false;
        nr_problem = reason;
        std::printf("DLSS: Neural Rendering unavailable (%.*s)\n", int(reason.size()), reason.data());
    }
};

Dlss* Dlss::Get() {
    static Dlss* const dlss = []() -> Dlss* {
        const char* setting = std::getenv("GOW3_DLSS");
        if (setting && setting[0] == '0') {
            return nullptr;
        }
        const auto directory = ExecutableDirectory();
        if (!std::filesystem::is_regular_file(directory / BridgeName) ||
            !std::filesystem::is_regular_file(directory / NgxName)) {
            return nullptr;
        }
        auto* created = new Dlss;
        // The process may end through quick_exit/_exit paths: release NGX first when it can.
        std::atexit([] {
            if (Dlss* live = Get()) {
                live->Shutdown();
            }
        });
        return created;
    }();
    return dlss;
}

Dlss::Dlss() : impl{std::make_unique<Impl>()} {
    const auto directory = ExecutableDirectory();
    impl->module = LoadLibraryW((directory / BridgeName).c_str());
    const auto get_api =
        impl->module ? reinterpret_cast<Gow3DlssGetApiFn>(GetProcAddress(impl->module, "Gow3DlssGetApi"))
                     : nullptr;
    impl->api = get_api ? get_api() : nullptr;
    if (!impl->api || impl->api->abi != GOW3_DLSS_BRIDGE_ABI) {
        impl->Disable("gow3_dlss.dll is missing or from another version");
        return;
    }
    // NGX writes its logs and model updates here: beside the saves and shader caches.
    std::filesystem::path data = directory / "dlss";
    if (const char* user = std::getenv("GOW3_GPU_USER_DIR"); user && user[0]) {
        data = std::filesystem::path{user} / "dlss";
    }
    std::error_code error;
    std::filesystem::create_directories(data, error);
    if (!impl->api->Configure(directory.c_str(), data.c_str(), BridgeLog)) {
        impl->Disable("bridge configuration failed");
        return;
    }
    // The Neural Rendering runtime is a separate DLL the user supplies. It is probed here, before
    // the device exists, because whether it is there decides which device extensions the model
    // needs. Nothing is created by a probe.
    if (impl->api->NrProbe(&impl->nr_entries)) {
        impl->nr_ready = true;
    } else {
        impl->nr_problem = "nvngx_dlssnr.dll (the DLSS 5 runtime) is not installed";
    }
}

Dlss::~Dlss() {
    Shutdown();
}

void Dlss::AppendInstanceExtensions(std::vector<const char*>& enabled) {
    if (!impl->eligible) {
        return;
    }
    u32 count{};
    const VkExtensionProperties* required{};
    if (!impl->api->InstanceExtensions(&count, &required)) {
        return impl->Disable("instance extension query failed");
    }
    const auto [result, extensions] = vk::enumerateInstanceExtensionProperties();
    if (result != vk::Result::eSuccess ||
        !CollectExtensions(required, count, extensions, impl->instance_extensions, enabled)) {
        return impl->Disable("required Vulkan instance extensions are missing");
    }
    impl->instance_ready = true;

    // The Neural Rendering model needs one instance extension of its own, and an instance's
    // extension list is fixed when the instance is created: asking afterwards is a permanent no.
    // It is only asked for when the model is actually wanted, and a missing one takes the model
    // down rather than Super Resolution with it.
    if (!impl->nr_ready || !Gow3Settings::Get().dlss_nr) {
        return;
    }
    u32 nr_count{};
    const VkExtensionProperties* nr_required{};
    if (!impl->api->NrInstanceExtensions(&nr_count, &nr_required)) {
        return impl->DisableNr("the runtime would not report its instance extensions");
    }
    std::vector<std::string>& nr_storage = impl->nr_instance_extensions;
    nr_storage.clear();
    for (u32 i = 0; i < nr_count; ++i) {
        const auto& request = nr_required[i];
        const auto match = std::ranges::find_if(extensions, [&](const auto& extension) {
            return std::strcmp(extension.extensionName.data(), request.extensionName) == 0 &&
                   extension.specVersion >= request.specVersion;
        });
        if (match == extensions.end()) {
            return impl->DisableNr(request.extensionName);
        }
        nr_storage.emplace_back(request.extensionName);
    }
    for (const auto& name : nr_storage) {
        if (std::ranges::none_of(enabled, [&](const char* current) { return name == current; })) {
            enabled.push_back(name.c_str());
        }
    }
}

void Dlss::AppendDeviceExtensions(vk::Instance instance, vk::PhysicalDevice physical,
                                  std::vector<const char*>& enabled) {
    if (!impl->eligible || !impl->instance_ready) {
        return;
    }
    const auto properties = physical.getProperties();
    if (properties.vendorID != 0x10de) {
        return impl->Disable("not an NVIDIA GPU");
    }
    u32 count{};
    const VkExtensionProperties* required{};
    if (!impl->api->DeviceExtensions(instance, physical, &count, &required)) {
        return impl->Disable("this GPU or driver does not support DLSS (GeForce RTX needed)");
    }
    const auto [result, extensions] = physical.enumerateDeviceExtensionProperties();
    if (result != vk::Result::eSuccess ||
        !CollectExtensions(required, count, extensions, impl->device_extensions, enabled)) {
        return impl->Disable("required Vulkan device extensions are missing");
    }
    impl->device_ready = true;

    // The Neural Rendering model needs extensions of its own, and a device's extension list is fixed
    // when the device is created: asking afterwards is a permanent no. They are only asked for when
    // the model is actually wanted, and a missing one takes the model down rather than Super
    // Resolution with it.
    if (!impl->nr_ready || !Gow3Settings::Get().dlss_nr) {
        return;
    }
    u32 nr_count{};
    const VkExtensionProperties* nr_required{};
    if (!impl->api->NrDeviceExtensions(instance, physical, &nr_count, &nr_required)) {
        return impl->DisableNr("the runtime would not report its device extensions");
    }
    std::vector<std::string>& nr_storage = impl->nr_device_extensions;
    nr_storage.clear();
    for (u32 i = 0; i < nr_count; ++i) {
        const auto& request = nr_required[i];
        const auto match = std::ranges::find_if(extensions, [&](const auto& extension) {
            return std::strcmp(extension.extensionName.data(), request.extensionName) == 0 &&
                   extension.specVersion >= request.specVersion;
        });
        if (match == extensions.end()) {
            return impl->DisableNr(request.extensionName);
        }
        nr_storage.emplace_back(request.extensionName);
    }
    for (const auto& name : nr_storage) {
        if (std::ranges::none_of(enabled, [&](const char* current) { return name == current; })) {
            enabled.push_back(name.c_str());
        }
    }
}

void Dlss::Initialize(vk::Instance instance, vk::PhysicalDevice physical, vk::Device device) {
    if (!impl->eligible || !impl->device_ready || impl->initialized) {
        return;
    }
    impl->initialized = true;
    if (!impl->api->Initialize(instance, physical, device,
                               VULKAN_HPP_DEFAULT_DISPATCHER.vkGetInstanceProcAddr,
                               VULKAN_HPP_DEFAULT_DISPATCHER.vkGetDeviceProcAddr)) {
        return impl->Disable("NVIDIA NGX initialization failed (update the GPU driver)");
    }
    impl->available = true;
    std::printf("DLSS: ready\n");
    // The Neural Rendering runtime is a second NGX, inside its own DLL, and it comes up only once
    // the device exists.
    if (impl->nr_ready && Gow3Settings::Get().dlss_nr) {
        if (!impl->api->NrInitialize(instance, physical, device)) {
            impl->DisableNr("the runtime would not initialise on this device");
        } else {
            impl->nr_available = true;
            impl->nr_initialized = true;
        }
    }
}

bool Dlss::Available() const {
    return impl->available;
}

std::string Dlss::Problem() const {
    return impl->problem;
}

bool Dlss::HasFeature(const FeatureDesc& desc) const {
    return impl->feature && *impl->feature == desc;
}

bool Dlss::CreateFeature(vk::CommandBuffer command, const FeatureDesc& desc) {
    if (!impl->available) {
        return false;
    }
    impl->feature.reset();
    const Gow3DlssFeature feature{desc.input_width,  desc.input_height, desc.output_width,
                                desc.output_height, desc.quality,      0,
                                0,                  desc.hdr ? 1 : 0};
    if (!impl->api->CreateFeature(command, &feature)) {
        return false;
    }
    impl->feature = desc;
    return true;
}

bool Dlss::Evaluate(vk::CommandBuffer command, const Frame& frame) {
    if (!impl->available || !impl->feature) {
        return false;
    }
    const auto image = [](const Resource& r) {
        const vk::ImageSubresourceRange range{r.aspect, 0, 1, 0, 1};
        return Gow3DlssImage{r.image,
                           r.view,
                           static_cast<VkImageSubresourceRange>(range),
                           static_cast<VkFormat>(r.format),
                           r.width,
                           r.height};
    };
    const Gow3DlssEvaluate parameters{image(frame.color),  image(frame.depth), image(frame.motion),
                                    image(frame.output), frame.jitter_x,     frame.jitter_y,
                                    frame.reset ? 1 : 0, frame.frame_ms,     frame.sharpness};
    // v18 forensics: bracket the NGX evaluate call (first 3) - if the entry print
    // appears without the exit print, the call itself never returns.
    static u32 eval_count = 0;
    const u32 eval_no = ++eval_count;
    if (eval_no <= 3) {
        std::printf("DLSS: evaluate #%u recording: color fmt=%d %ux%u aspect=%u, depth fmt=%d %ux%u aspect=%u, motion fmt=%d %ux%u, output fmt=%d %ux%u, reset=%d\n",
                    eval_no, static_cast<int>(frame.color.format), frame.color.width,
                    frame.color.height, static_cast<u32>(frame.color.aspect),
                    static_cast<int>(frame.depth.format), frame.depth.width, frame.depth.height,
                    static_cast<u32>(frame.depth.aspect), static_cast<int>(frame.motion.format),
                    frame.motion.width, frame.motion.height, static_cast<int>(frame.output.format),
                    frame.output.width, frame.output.height, frame.reset ? 1 : 0);
    }
    const bool eval_ok = impl->api->Evaluate(command, &parameters) != 0;
    if (eval_no <= 3) {
        std::printf("DLSS: evaluate #%u returned %d\n", eval_no, eval_ok ? 1 : 0);
    }
    return eval_ok;
}

void Dlss::ReleaseFeature() {
    impl->feature.reset();
    if (impl->api) {
        impl->api->ReleaseFeature();
    }
}

bool Dlss::NrInstalled() const {
    return impl->nr_ready;
}

bool Dlss::NrAvailable() const {
    return impl->nr_available;
}

std::string Dlss::NrProblem() const {
    return impl->nr_problem;
}

bool Dlss::HasNrFeature(const NrFeatureDesc& desc) const {
    return impl->nr_feature && *impl->nr_feature == desc;
}

bool Dlss::CreateNrFeature(vk::CommandBuffer command, const NrFeatureDesc& desc) {
    if (!impl->nr_available) {
        return false;
    }
    impl->nr_feature.reset();
    const Gow3DlssNrFeature feature{desc.input_width,  desc.input_height, desc.output_width,
                                    desc.output_height, desc.preset,      desc.intensity,
                                    desc.style,       desc.local_structure, desc.local_tone,
                                    desc.skin_structure, desc.auto_mask ? 1 : 0,
                                    desc.ui_correction ? 1 : 0, desc.depth_inverted ? 1 : 0};
    if (!impl->api->NrCreateFeature(command, &feature)) {
        return false;
    }
    impl->nr_feature = desc;
    return true;
}

bool Dlss::EvaluateNr(vk::CommandBuffer command, const NrFrame& frame) {
    if (!impl->nr_available || !impl->nr_feature) {
        return false;
    }
    const auto image = [](const Resource& r) {
        const vk::ImageSubresourceRange range{r.aspect, 0, 1, 0, 1};
        return Gow3DlssImage{r.image,
                           r.view,
                           static_cast<VkImageSubresourceRange>(range),
                           static_cast<VkFormat>(r.format),
                           r.width,
                           r.height};
    };
    const Gow3DlssNrFrame parameters{
        image(frame.color),  image(frame.depth),  image(frame.motion),  image(frame.output),
        frame.guide_width,   frame.guide_height, frame.motion_width,  frame.motion_height,
        frame.depth_base_x,  frame.depth_base_y, frame.motion_base_x, frame.motion_base_y,
        frame.depth_inverted ? 1 : 0, frame.reset ? 1 : 0, frame.intensity, frame.style,
        frame.local_structure, frame.local_tone, frame.skin_structure, frame.auto_mask ? 1 : 0,
        frame.mv_scale_x,    frame.mv_scale_y};
    return impl->api->NrEvaluate(command, &parameters) != 0;
}

void Dlss::ReleaseNrFeature() {
    impl->nr_feature.reset();
    if (impl->api) {
        impl->api->NrReleaseFeature();
    }
}

void Dlss::Shutdown() {
    impl->available = false;
    impl->feature.reset();
    if (impl->api && impl->initialized) {
        impl->api->Shutdown();
        impl->initialized = false;
    }
}

#else

struct Dlss::Impl {};
Dlss::Dlss() = default;
Dlss::~Dlss() = default;
Dlss* Dlss::Get() {
    return nullptr;
}
void Dlss::AppendInstanceExtensions(std::vector<const char*>&) {}
void Dlss::AppendDeviceExtensions(vk::Instance, vk::PhysicalDevice, std::vector<const char*>&) {}
void Dlss::Initialize(vk::Instance, vk::PhysicalDevice, vk::Device) {}
void Dlss::Shutdown() {}
bool Dlss::Available() const {
    return false;
}
std::string Dlss::Problem() const {
    return {};
}
bool Dlss::HasFeature(const FeatureDesc&) const {
    return false;
}
bool Dlss::CreateFeature(vk::CommandBuffer, const FeatureDesc&) {
    return false;
}
bool Dlss::Evaluate(vk::CommandBuffer, const Frame&) {
    return false;
}
void Dlss::ReleaseFeature() {}
bool Dlss::NrAvailable() const {
    return false;
}
bool Dlss::NrInstalled() const {
    return false;
}
std::string Dlss::NrProblem() const {
    return {};
}
bool Dlss::HasNrFeature(const NrFeatureDesc&) const {
    return false;
}
bool Dlss::CreateNrFeature(vk::CommandBuffer, const NrFeatureDesc&) {
    return false;
}
bool Dlss::EvaluateNr(vk::CommandBuffer, const NrFrame&) {
    return false;
}
void Dlss::ReleaseNrFeature() {}

#endif

} // namespace Vulkan
