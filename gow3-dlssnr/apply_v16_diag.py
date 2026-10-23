#!/usr/bin/env python3
"""v16: the scene-color format bridge for TemporalUpscaler::Run().

v15's log proved the whole upstream chain works: the view-proj feed is active
(f=1.968 g=3.500 A=-1.0000 B=-0.4996), the scene depth feed sets the depth,
the order-based trigger fires, and Run() executes - then skips on exactly one
condition: "color storage usage=1, color format=43" (R8G8B8A8Srgb) vs its
RGBA16F requirement. Depth size and storage usage already pass.

v16 bridges the format inside Run():
- the gate accepts R8G8B8A8Srgb / R8G8B8A8Unorm as well as RGBA16F;
- when the scene color is not RGBA16F (and the frame is not the reduced-size
  path), Run() copies the guest target into an owned RGBA16F image
  (bridge_image: Storage | Sampled | TransferSrc | TransferDst) after the
  layout transits, and uses it for color_view / input_color / input_format
  everywhere downstream (reactive, DLSS - including the NR pass inside
  RecordDlss - FSR, and the merge pass's scene target);
- after the merge dispatch writes the upscaled result into the bridge, Run()
  copies it back into the guest scene target, which is what the game's
  composite pass reads.
Copies are raw texel copies (the sRGB-encoded values are treated as linear
downstream - a gamma shift, acceptable for first light). All barriers keep
both images in eGeneral so the texture cache's layout tracking for the guest
image stays truthful.
"""
import sys

def detect_nl(content):
    return "\r\n" if "\r\n" in content else "\n"

def apply(path, edits):
    with open(path, "r", newline="") as f:
        content = f.read()
    nl = detect_nl(content)
    for name, old_lines, new_lines in edits:
        old = nl.join(old_lines)
        new = nl.join(new_lines)
        n = content.count(old)
        if n != 1:
            print(f"FAIL {path} :: {name}: anchor found {n} times")
            sys.exit(1)
        content = content.replace(old, new)
        print(f"ok   {path} :: {name}")
    with open(path, "w", newline="") as f:
        f.write(content)

TUH = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.h"
TU = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.cpp"

# --- H1: bridge members ---------------------------------------------------------
old_h1 = [
    "    vk::UniqueImageView opaque_view;",
    "    vk::UniqueImageView reactive_view;",
]
new_h1 = [
    "    vk::UniqueImageView opaque_view;",
    "    vk::UniqueImageView reactive_view;",
    "    VideoCore::UniqueImage bridge_image; ///< v16: RGBA16F copy of an 8-bit scene target",
    "    vk::UniqueImageView bridge_view;",
    "    u32 bridge_width = 0;",
    "    u32 bridge_height = 0;",
]

# --- T1: the Run() gate accepts the 8-bit scene formats -------------------------
old_t1 = [
    "    if (color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat ||",
    "        depth.info.size.width != ow || depth.info.size.height != oh ||",
    "        !(color.usage_flags & vk::ImageUsageFlagBits::eStorage)) {",
]
new_t1 = [
    "    // v16: this build's scene target is 8-bit (R8G8B8A8Srgb); it is bridged to an",
    "    // owned RGBA16F image below instead of being rejected here.",
    "    const bool bridged = !reduced && color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat;",
    "    if ((color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat &&",
    "         color.info.pixel_format != vk::Format::eR8G8B8A8Srgb &&",
    "         color.info.pixel_format != vk::Format::eR8G8B8A8Unorm) ||",
    "        depth.info.size.width != ow || depth.info.size.height != oh ||",
    "        !(color.usage_flags & vk::ImageUsageFlagBits::eStorage)) {",
]

# --- T2: bridge creation + color_view selection ----------------------------------
old_t2 = [
    "    const auto color_view =",
    "        CachedView(color, vk::Format::eR16G16B16A16Sfloat, vk::ImageAspectFlagBits::eColor);",
]
new_t2 = [
    "    if (bridged) {",
    "        const auto device = instance.GetDevice();",
    "        if (!bridge_image || bridge_width != ow || bridge_height != oh) {",
    "            bridge_view.reset();",
    "            bridge_image = VideoCore::UniqueImage(device, instance.GetAllocator());",
    "            bridge_image.Create(vk::ImageCreateInfo{",
    "                .imageType = vk::ImageType::e2D,",
    "                .format = vk::Format::eR16G16B16A16Sfloat,",
    "                .extent = {ow, oh, 1},",
    "                .mipLevels = 1,",
    "                .arrayLayers = 1,",
    "                .samples = vk::SampleCountFlagBits::e1,",
    "                .tiling = vk::ImageTiling::eOptimal,",
    "                .usage = vk::ImageUsageFlagBits::eStorage | vk::ImageUsageFlagBits::eSampled |",
    "                         vk::ImageUsageFlagBits::eTransferSrc |",
    "                         vk::ImageUsageFlagBits::eTransferDst,",
    "                .initialLayout = vk::ImageLayout::eUndefined,",
    "            });",
    "            bridge_view = Check(device.createImageViewUnique({",
    "                .image = vk::Image(bridge_image),",
    "                .viewType = vk::ImageViewType::e2D,",
    "                .format = vk::Format::eR16G16B16A16Sfloat,",
    "                .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1},",
    "            }));",
    "            bridge_width = ow;",
    "            bridge_height = oh;",
    '            std::printf("Upscaler: v16 scene bridge: %ux%u format %d -> RGBA16F '
                '(guest usage=%u)\\n",',
    "                        ow, oh, static_cast<int>(color.info.pixel_format),",
    "                        static_cast<u32>(color.usage_flags));",
    "        }",
    "    }",
    "    const auto color_view =",
    "        bridged ? *bridge_view",
    "                : CachedView(color, vk::Format::eR16G16B16A16Sfloat,",
    "                             vk::ImageAspectFlagBits::eColor);",
]

# --- T3: copy-in before RecordMotion + input selection ---------------------------
old_t3 = [
    "    camera_motion.RecordMotion(cmdbuf, input_depth_view, *motion_view, w, h);",
]
new_t3 = [
    "    const vk::Format input_format = bridged ? vk::Format::eR16G16B16A16Sfloat",
    "                                            : color.info.pixel_format;",
    "    const vk::ImageUsageFlags input_usage =",
    "        bridged ? vk::ImageUsageFlagBits::eStorage | vk::ImageUsageFlagBits::eSampled |",
    "                      vk::ImageUsageFlagBits::eTransferSrc |",
    "                      vk::ImageUsageFlagBits::eTransferDst",
    "                : color.usage_flags;",
    "    if (bridged) {",
    "        // v16: copy the guest scene color into the RGBA16F bridge (raw texel copy).",
    "        // All layouts stay eGeneral, matching the texture cache's tracking.",
    "        const vk::Image bridge = vk::Image(bridge_image);",
    "        const vk::ImageCopy region{",
    "            .srcSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "            .dstSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "            .extent = {ow, oh, 1},",
    "        };",
    "        const std::array<vk::ImageMemoryBarrier2, 2> to_transfer = {{",
    "            {.srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,",
    "             .srcAccessMask = vk::AccessFlagBits2::eShaderRead | vk::AccessFlagBits2::eShaderWrite,",
    "             .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "             .dstAccessMask = vk::AccessFlagBits2::eTransferRead,",
    "             .oldLayout = vk::ImageLayout::eGeneral,",
    "             .newLayout = vk::ImageLayout::eGeneral,",
    "             .image = input_color,",
    "             .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}},",
    "            {.srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,",
    "             .srcAccessMask = vk::AccessFlagBits2::eShaderRead | vk::AccessFlagBits2::eShaderWrite,",
    "             .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "             .dstAccessMask = vk::AccessFlagBits2::eTransferWrite,",
    "             .oldLayout = vk::ImageLayout::eGeneral,",
    "             .newLayout = vk::ImageLayout::eGeneral,",
    "             .image = bridge,",
    "             .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}},",
    "        }};",
    "        cmdbuf.pipelineBarrier2({.imageMemoryBarrierCount = 2,",
    "                                 .pImageMemoryBarriers = to_transfer.data()});",
    "        cmdbuf.copyImage(input_color, vk::ImageLayout::eGeneral, bridge,",
    "                         vk::ImageLayout::eGeneral, region);",
    "        const vk::ImageMemoryBarrier2 to_read{",
    "            .srcStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "            .srcAccessMask = vk::AccessFlagBits2::eTransferWrite,",
    "            .dstStageMask = vk::PipelineStageFlagBits2::eComputeShader,",
    "            .dstAccessMask = vk::AccessFlagBits2::eShaderRead | vk::AccessFlagBits2::eShaderWrite,",
    "            .oldLayout = vk::ImageLayout::eGeneral,",
    "            .newLayout = vk::ImageLayout::eGeneral,",
    "            .image = bridge,",
    "            .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}};",
    "        cmdbuf.pipelineBarrier2({.imageMemoryBarrierCount = 1, .pImageMemoryBarriers = &to_read});",
    "        input_color = bridge;",
    "        input_color_view = *bridge_view;",
    "    }",
    "    camera_motion.RecordMotion(cmdbuf, input_depth_view, *motion_view, w, h);",
]

# --- T4: DLSS describe uses the input format --------------------------------------
old_t4 = [
    "            {input_color, input_color_view, color.info.pixel_format, vk::ImageAspectFlagBits::eColor, w, h},",
]
new_t4 = [
    "            {input_color, input_color_view, input_format, vk::ImageAspectFlagBits::eColor, w, h},",
]

# --- T5: FSR describe uses the input format/usage ----------------------------------
old_t5 = [
    "        info.color = Describe(input_color, color.info.pixel_format, w, h,",
    "                              color.usage_flags, vk::ImageAspectFlagBits::eColor,",
]
new_t5 = [
    "        info.color = Describe(input_color, input_format, w, h,",
    "                              input_usage, vk::ImageAspectFlagBits::eColor,",
]

# --- T6: copy-back after the merge dispatch -----------------------------------------
old_t6 = [
    "        cmdbuf.pushConstants(*merge_pipeline_layout, vk::ShaderStageFlagBits::eCompute, 0,",
    "                             sizeof(mode), &mode);",
    "        cmdbuf.dispatch((ow + 7) / 8, (oh + 7) / 8, 1);",
    "    }",
    "}",
]
new_t6 = [
    "        cmdbuf.pushConstants(*merge_pipeline_layout, vk::ShaderStageFlagBits::eCompute, 0,",
    "                             sizeof(mode), &mode);",
    "        cmdbuf.dispatch((ow + 7) / 8, (oh + 7) / 8, 1);",
    "        if (bridged) {",
    "            // v16: the merge wrote the upscaled result into the bridge; copy it back",
    "            // into the guest scene target, which is what the game's composite reads.",
    "            const vk::Image bridge = vk::Image(bridge_image);",
    "            const vk::Image guest = color.GetImage();",
    "            const vk::ImageCopy region{",
    "                .srcSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "                .dstSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "                .extent = {ow, oh, 1},",
    "            };",
    "            const std::array<vk::ImageMemoryBarrier2, 2> to_transfer = {{",
    "                {.srcStageMask = vk::PipelineStageFlagBits2::eComputeShader,",
    "                 .srcAccessMask = vk::AccessFlagBits2::eShaderWrite,",
    "                 .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "                 .dstAccessMask = vk::AccessFlagBits2::eTransferRead,",
    "                 .oldLayout = vk::ImageLayout::eGeneral,",
    "                 .newLayout = vk::ImageLayout::eGeneral,",
    "                 .image = bridge,",
    "                 .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}},",
    "                {.srcStageMask = vk::PipelineStageFlagBits2::eComputeShader,",
    "                 .srcAccessMask = vk::AccessFlagBits2::eShaderRead | vk::AccessFlagBits2::eShaderWrite,",
    "                 .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "                 .dstAccessMask = vk::AccessFlagBits2::eTransferWrite,",
    "                 .oldLayout = vk::ImageLayout::eGeneral,",
    "                 .newLayout = vk::ImageLayout::eGeneral,",
    "                 .image = guest,",
    "                 .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}},",
    "            }};",
    "            cmdbuf.pipelineBarrier2({.imageMemoryBarrierCount = 2,",
    "                                     .pImageMemoryBarriers = to_transfer.data()});",
    "            cmdbuf.copyImage(bridge, vk::ImageLayout::eGeneral, guest,",
    "                             vk::ImageLayout::eGeneral, region);",
    "            const vk::ImageMemoryBarrier2 done{",
    "                .srcStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "                .srcAccessMask = vk::AccessFlagBits2::eTransferWrite,",
    "                .dstStageMask = vk::PipelineStageFlagBits2::eAllCommands,",
    "                .dstAccessMask = vk::AccessFlagBits2::eShaderRead | vk::AccessFlagBits2::eShaderWrite,",
    "                .oldLayout = vk::ImageLayout::eGeneral,",
    "                .newLayout = vk::ImageLayout::eGeneral,",
    "                .image = guest,",
    "                .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}};",
    "            cmdbuf.pipelineBarrier2({.imageMemoryBarrierCount = 1, .pImageMemoryBarriers = &done});",
    "        }",
    "    }",
    "}",
]

apply(TUH, [("H1 bridge members", old_h1, new_h1)])
apply(TU, [
    ("T1 gate accepts 8-bit", old_t1, new_t1),
    ("T2 bridge creation + view", old_t2, new_t2),
    ("T3 copy-in + input selection", old_t3, new_t3),
    ("T4 DLSS input format", old_t4, new_t4),
    ("T5 FSR input format/usage", old_t5, new_t5),
    ("T6 copy-back after merge", old_t6, new_t6),
])
print("all v16 edits applied")
