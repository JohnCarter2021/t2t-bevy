#!/usr/bin/env python3
"""v17: extend the v16 scene-color bridge to the reduced path.

Bisect result (his last_run-11.log): with Neural Rendering OFF the game still
froze, right after "DLSS: DLSS 1280x720 -> 1920x1080, quality 1, preset 0, HDR
input" - so the hang is the core SR evaluate, not NR.

Root cause, found in gpu/dlss_bridge/gow3_dlss_bridge.cpp: Evaluate() wraps
every resource with NVSDK_NGX_Create_ImageView_Resource_VK using the format
it is handed, verbatim. Run()'s reduced path (the one this build actually
takes: the scene proxy from scene_targets.Read at 1280x720) describes the
color as R8G8B8A8Srgb (input_format = color.info.pixel_format when not
bridged). DLSS does not accept an sRGB-typed input color. Feature creation
never touches resource formats (hence its success print); evaluate is the
first consumer of the format - hence the hang there.

v16's bridge only covered the NON-reduced path (bridged = !reduced && ...),
which this build never takes. v17 changes, all in Run():
- bridged = (color format != RGBA16F), regardless of reduced;
- the bridge image is sized to the INPUT size (w, h) - the render size -
  not the output size, and the copy-in extent follows it; the copy source is
  whatever input_color holds at that point (the scene proxy when reduced,
  the guest target when not);
- color_view stays the CachedView of the guest target when reduced (the
  merge pass writes the upscaled output into the guest target there, as the
  author designed); the bridge feeds only the input side;
- the copy-back runs only when NOT reduced (in reduced mode the bridge is
  never the merge target);
- the copy-in barriers' source access masks are widened to MemoryRead |
  MemoryWrite (the proxy's last writer is scene_targets' transfer copy).
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

TU = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.cpp"

old_e1 = [
    "    // v16: this build's scene target is 8-bit (R8G8B8A8Srgb); it is bridged to an",
    "    // owned RGBA16F image below instead of being rejected here.",
    "    const bool bridged = !reduced && color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat;",
]
new_e1 = [
    "    // v16: this build's scene target is 8-bit (R8G8B8A8Srgb); it is bridged to an",
    "    // owned RGBA16F image below instead of being rejected here.",
    "    // v17: the bridge also covers the reduced path - v16 skipped it there, and the",
    "    // scene proxy reached DLSS still described as sRGB, which NGX does not accept",
    "    // as an input color format; the first evaluate hung on it.",
    "    const bool bridged = color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat;",
]

old_e2a = [
    "            bridge_image.Create(vk::ImageCreateInfo{",
    "                .imageType = vk::ImageType::e2D,",
    "                .format = vk::Format::eR16G16B16A16Sfloat,",
    "                .extent = {ow, oh, 1},",
]
new_e2a = [
    "            bridge_image.Create(vk::ImageCreateInfo{",
    "                .imageType = vk::ImageType::e2D,",
    "                .format = vk::Format::eR16G16B16A16Sfloat,",
    "                .extent = {w, h, 1},",
]
old_e2b = [
    "            bridge_width = ow;",
    "            bridge_height = oh;",
]
new_e2b = [
    "            bridge_width = w;",
    "            bridge_height = h;",
]
old_e2c = [
    "        if (!bridge_image || bridge_width != ow || bridge_height != oh) {",
]
new_e2c = [
    "        if (!bridge_image || bridge_width != w || bridge_height != h) {",
]
old_e2d = [
    "                        ow, oh, static_cast<int>(color.info.pixel_format),",
]
new_e2d = [
    "                        w, h, static_cast<int>(color.info.pixel_format),",
]

old_e3 = [
    "    const auto color_view =",
    "        bridged ? *bridge_view",
    "                : CachedView(color, vk::Format::eR16G16B16A16Sfloat,",
    "                             vk::ImageAspectFlagBits::eColor);",
]
new_e3 = [
    "    // v17: when reduced, the merge pass still writes into the GUEST target (the",
    "    // author's design), so color_view stays its CachedView; the bridge view is",
    "    // the scene target only in the non-reduced path. The DLSS/FSR input always",
    "    // comes from the bridge via input_color_view (set in the copy-in below).",
    "    const auto color_view =",
    "        (bridged && !reduced) ? *bridge_view",
    "                              : CachedView(color, vk::Format::eR16G16B16A16Sfloat,",
    "                                           vk::ImageAspectFlagBits::eColor);",
]

old_e4 = [
    "        const vk::Image bridge = vk::Image(bridge_image);",
    "        const vk::ImageCopy region{",
    "            .srcSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "            .dstSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "            .extent = {ow, oh, 1},",
    "        };",
]
new_e4 = [
    "        const vk::Image bridge = vk::Image(bridge_image);",
    "        const vk::ImageCopy region{",
    "            .srcSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "            .dstSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},",
    "            .extent = {w, h, 1},",
    "        };",
]

# widen the copy-in source access masks (both barriers of the to_transfer array,
# identified by the 8-space indentation unique to the copy-in block)
old_e5 = [
    "        const std::array<vk::ImageMemoryBarrier2, 2> to_transfer = {{",
    "            {.srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,",
    "             .srcAccessMask = vk::AccessFlagBits2::eShaderRead | vk::AccessFlagBits2::eShaderWrite,",
    "             .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "             .dstAccessMask = vk::AccessFlagBits2::eTransferRead,",
]
new_e5 = [
    "        const std::array<vk::ImageMemoryBarrier2, 2> to_transfer = {{",
    "            {.srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,",
    "             .srcAccessMask = vk::AccessFlagBits2::eMemoryRead | vk::AccessFlagBits2::eMemoryWrite,",
    "             .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "             .dstAccessMask = vk::AccessFlagBits2::eTransferRead,",
]
old_e6 = [
    "            {.srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,",
    "             .srcAccessMask = vk::AccessFlagBits2::eShaderRead | vk::AccessFlagBits2::eShaderWrite,",
    "             .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "             .dstAccessMask = vk::AccessFlagBits2::eTransferWrite,",
]
new_e6 = [
    "            {.srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,",
    "             .srcAccessMask = vk::AccessFlagBits2::eMemoryRead | vk::AccessFlagBits2::eMemoryWrite,",
    "             .dstStageMask = vk::PipelineStageFlagBits2::eTransfer,",
    "             .dstAccessMask = vk::AccessFlagBits2::eTransferWrite,",
]

old_e7 = [
    "        cmdbuf.dispatch((ow + 7) / 8, (oh + 7) / 8, 1);",
    "        if (bridged) {",
]
new_e7 = [
    "        cmdbuf.dispatch((ow + 7) / 8, (oh + 7) / 8, 1);",
    "        if (bridged && !reduced) {",
]

apply(TU, [
    ("E1 bridged covers reduced", old_e1, new_e1),
    ("E2a bridge extent = input size", old_e2a, new_e2a),
    ("E2b bridge size tracking", old_e2b, new_e2b),
    ("E2c bridge recreate check", old_e2c, new_e2c),
    ("E2d bridge print size", old_e2d, new_e2d),
    ("E3 color_view in reduced mode", old_e3, new_e3),
    ("E4 copy-in extent = input size", old_e4, new_e4),
    ("E5 copy-in src barrier 1 widened", old_e5, new_e5),
    ("E6 copy-in src barrier 2 widened", old_e6, new_e6),
    ("E7 copy-back only when not reduced", old_e7, new_e7),
])
print("all v17 edits applied")
