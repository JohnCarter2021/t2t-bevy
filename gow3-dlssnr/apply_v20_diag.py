#!/usr/bin/env python3
"""v20: route the reduced-path merge through an owned FP16 image.

Evidence chain: v18 forensics proved the DLSS evaluate returns success and
Run() completes recording; v19's own-submission creation changed nothing;
and the same v19 build FREEZES WITH FSR 3 TOO (his last_run-14.log: "FSR 3
context 1280x720 -> 1920x1080", bridge print, "Run completed #1", hang) -
so the fault is in the code both backends share, executed for the first
time on the first upscaled frame.

Audit of that shared path for Vulkan format contract violations:
- camera_motion.comp: motion write rg16f == R16G16Sfloat image OK;
  depth/object are samplerless textures (format-agnostic) OK.
- taa_sharpen.comp (ExtraSharpen): reads its own RGBA32F copy (rgba32f OK),
  writes RGBA16F output (rgba16f OK) - and it no-ops unless sharpness > 1.
- upscale_merge.comp: binding 1 `scene` is declared rgba16f and WRITTEN.
  In the NON-reduced path v16 already merges into the FP16 bridge (fine).
  In the REDUCED path - the path this game always takes - the merge binds
  the GUEST scene target, which is R8G8B8A8Srgb. A storage-image write
  through a mismatched declared format is undefined behavior and a classic
  first-dispatch GPU fault/hang.

v20 therefore gives the reduced path the same treatment at output size:
when the guest target is not FP16, copy it into an owned RGBA16F merge
image (preserving the alpha the merge keeps), merge there, and copy back -
reusing the existing copy-back block, generalized to either source image.
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

HDR = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.h"
TU = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.cpp"

old_h1 = [
    "    VideoCore::UniqueImage bridge_image; ///< v16: RGBA16F copy of an 8-bit scene target",
    "    vk::UniqueImageView bridge_view;",
    "    u32 bridge_width = 0;",
    "    u32 bridge_height = 0;",
]
new_h1 = old_h1 + [
    "    // v20: FP16 merge target for the reduced path, whose merge would otherwise",
    "    // write into the 8-bit guest target through the merge shader's rgba16f",
    "    // declaration (a storage-image format mismatch).",
    "    VideoCore::UniqueImage merge_image;",
    "    vk::UniqueImageView merge_view;",
    "    u32 merge_width = 0, merge_height = 0;",
]

old_t1 = [
    "        runtime.FlushBarriers();",
    "        const vk::DescriptorImageInfo out_info{.imageView = *output_view,",
    "                                               .imageLayout = vk::ImageLayout::eGeneral};",
    "        const vk::DescriptorImageInfo scene_info{.imageView = color_view,",
]
new_t1 = [
    "        runtime.FlushBarriers();",
    "        // v20: in the reduced path the merge would write into the guest target",
    "        // itself - an sRGB image the merge shader declares as rgba16f, a storage",
    "        // format mismatch that faults the GPU on the first upscaled frame (both",
    "        // DLSS and FSR hung there; the evaluate itself returned success). Merge",
    "        // into an owned FP16 image instead and copy back below.",
    "        const bool use_merge_copy =",
    "            reduced && color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat;",
    "        vk::ImageView scene_view = color_view;",
    "        if (use_merge_copy) {",
    "            if (merge_width != ow || merge_height != oh) {",
    "                scheduler.WaitSubmitted();",
    "                merge_view.reset();",
    "                merge_image = VideoCore::UniqueImage(instance.GetDevice(), instance.GetAllocator());",
    "                merge_image.Create(vk::ImageCreateInfo{",
    "                    .imageType = vk::ImageType::e2D,",
    "                    .format = vk::Format::eR16G16B16A16Sfloat,",
    "                    .extent = {ow, oh, 1},",
    "                    .mipLevels = 1, .arrayLayers = 1, .samples = vk::SampleCountFlagBits::e1,",
    "                    .tiling = vk::ImageTiling::eOptimal,",
    "                    .usage = vk::ImageUsageFlagBits::eStorage | vk::ImageUsageFlagBits::eTransferSrc |",
    "                             vk::ImageUsageFlagBits::eTransferDst,",
    "                    .initialLayout = vk::ImageLayout::eUndefined,",
    "                });",
    "                merge_view = Check(instance.GetDevice().createImageViewUnique({",
    "                    .image = vk::Image(merge_image), .viewType = vk::ImageViewType::e2D,",
    "                    .format = vk::Format::eR16G16B16A16Sfloat,",
    "                    .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1},",
    "                }));",
    "                merge_width = ow;",
    "                merge_height = oh;",
    "            }",
    "            scene_view = *merge_view;",
    "        }",
    "        const vk::DescriptorImageInfo out_info{.imageView = *output_view,",
    "                                               .imageLayout = vk::ImageLayout::eGeneral};",
    "        const vk::DescriptorImageInfo scene_info{.imageView = scene_view,",
]

old_t2 = [
    "        // Debug (menu or toggle 1 << 28): the reactive mask in red over a darkened frame;",
]
new_t2 = [
    "        if (use_merge_copy) {",
    "            // Copy the guest target into the merge image first (the merge keeps",
    "            // the scene alpha, which carries data for the post chain).",
    "            const vk::Image guest = color.GetImage();",
    "            const vk::Image merge_img = vk::Image(merge_image);",
    "            const vk::ImageCopy region{",
    "                .srcSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "                .dstSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "                .extent = {ow, oh, 1},",
    "            };",
    "            const std::array<vk::ImageMemoryBarrier2, 2> to_copy = {{",
    "                {.srcStageMask = all,",
    "                 .srcAccessMask = rw,",
    "                 .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "                 .dstAccessMask = vk::AccessFlagBits2::eTransferRead,",
    "                 .oldLayout = vk::ImageLayout::eGeneral,",
    "                 .newLayout = vk::ImageLayout::eGeneral,",
    "                 .image = guest,",
    "                 .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}},",
    "                {.srcStageMask = all,",
    "                 .srcAccessMask = vk::AccessFlagBits2::eNone,",
    "                 .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "                 .dstAccessMask = vk::AccessFlagBits2::eTransferWrite,",
    "                 .oldLayout = vk::ImageLayout::eUndefined,",
    "                 .newLayout = vk::ImageLayout::eGeneral,",
    "                 .image = merge_img,",
    "                 .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}},",
    "            }};",
    "            cmdbuf.pipelineBarrier2({.imageMemoryBarrierCount = 2,",
    "                                     .pImageMemoryBarriers = to_copy.data()});",
    "            cmdbuf.copyImage(guest, vk::ImageLayout::eGeneral, merge_img,",
    "                             vk::ImageLayout::eGeneral, region);",
    "            const std::array<vk::ImageMemoryBarrier2, 2> from_copy = {{",
    "                {.srcStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "                 .srcAccessMask = vk::AccessFlagBits2::eTransferWrite,",
    "                 .dstStageMask = vk::PipelineStageFlagBits2::eComputeShader,",
    "                 .dstAccessMask = vk::AccessFlagBits2::eShaderRead | vk::AccessFlagBits2::eShaderWrite,",
    "                 .oldLayout = vk::ImageLayout::eGeneral,",
    "                 .newLayout = vk::ImageLayout::eGeneral,",
    "                 .image = merge_img,",
    "                 .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}},",
    "                {.srcStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "                 .srcAccessMask = vk::AccessFlagBits2::eTransferRead,",
    "                 .dstStageMask = all,",
    "                 .dstAccessMask = rw,",
    "                 .oldLayout = vk::ImageLayout::eGeneral,",
    "                 .newLayout = vk::ImageLayout::eGeneral,",
    "                 .image = guest,",
    "                 .subresourceRange = {vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1}},",
    "            }};",
    "            cmdbuf.pipelineBarrier2({.imageMemoryBarrierCount = 2,",
    "                                     .pImageMemoryBarriers = from_copy.data()});",
    "            static bool merge_printed = false;",
    "            if (!merge_printed) {",
    "                merge_printed = true;",
    '                std::printf("Upscaler: v20 merge via FP16 copy (guest format %d)\\n",',
    "                            static_cast<int>(color.info.pixel_format));",
    "            }",
    "        }",
    "        // Debug (menu or toggle 1 << 28): the reactive mask in red over a darkened frame;",
]

old_t3 = [
    "        if (bridged && !reduced) {",
    "            // v16: the merge wrote the upscaled result into the bridge; copy it back",
    "            // into the guest scene target, which is what the game's composite reads.",
    "            const vk::Image bridge = vk::Image(bridge_image);",
    "            const vk::Image guest = color.GetImage();",
]
new_t3 = [
    "        if ((bridged && !reduced) || use_merge_copy) {",
    "            // v16/v20: the merge wrote the upscaled result into an owned FP16 image",
    "            // (the input bridge, or the v20 merge copy on the reduced path); copy it",
    "            // back into the guest scene target, which is what the game's composite reads.",
    "            const vk::Image bridge =",
    "                (bridged && !reduced) ? vk::Image(bridge_image) : vk::Image(merge_image);",
    "            const vk::Image guest = color.GetImage();",
]

apply(HDR, [("H1 merge members", old_h1, new_h1)])
apply(TU, [
    ("T1 merge target selection", old_t1, new_t1),
    ("T2 merge copy-in", old_t2, new_t2),
    ("T3 generalized copy-back", old_t3, new_t3),
])
print("all v20 edits applied")
