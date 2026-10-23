#!/usr/bin/env python3
"""v15: the camera feed rewrite, from the v14 dump evidence.

The v14 dumps proved (verified numerically: 60/68 object positions land in
the clip cube under this reading, f/g ratio = 1.777 = 16:9) that the game's
144-byte per-draw constant buffer holds, at floats [0..15], the frame's
view-projection matrix in column-major storage, the object's world matrix at
[16..31], and flags at [32..35].

This change set:
- CameraMotion::OnViewProj(data): decomposes the stored VP - logical rows
  r0..r3 (r = {data[r], data[4+r], data[8+r], data[12+r]}); f = |r0.xyz|,
  g = |r1.xyz|; view rows = r0/f, r1/g, -r3 (translations likewise);
  proj = {f, g, A, B} with A = r2.xyz . view_row2, B = r2[3] - A * view_t2;
  inv_view = rigid inverse (R^T, -R^T t). First feed per frame wins, the
  same heuristic OnConstants used. Plus SetRenderSize(w, h).
- vk_rasterizer: feed OnViewProj from every 144-byte vsharp binding; set the
  scene depth (via the existing OnGBufferPass setter) + render size from the
  first full-size scene draw (<=2 MRTs, sRGB or RGBA16F, matching sizes) -
  this build has no G-buffer pass, so nothing else ever sets the depth; the
  scene-color gate also accepts eR8G8B8A8Srgb so OnSceneColor fires (Run()
  keeps its own RGBA16F gate and reports its skip reason).
- vk_temporal_upscaler::OnDispatch: when GOW3_UPSCALE_BEFORE_CS is not set,
  also fire on the first compute dispatch that arrives with scene_color set
  and the camera Ready (the game's composite dispatch). The shipped trigger
  hash 0x9a9cf8a9 belongs to another build and never occurs here.
All v9-v14 diagnostics stay in place.
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

CMH = "gpu/shadps4/video_core/renderer_vulkan/vk_camera_motion.h"
CMC = "gpu/shadps4/video_core/renderer_vulkan/vk_camera_motion.cpp"
RZ = "gpu/shadps4/video_core/renderer_vulkan/vk_rasterizer.cpp"
TU = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.cpp"

# --- H1: declarations ----------------------------------------------------------
old_h1 = [
    "    /// A bound constant buffer of 864 bytes: checks the scene constant signature.",
    "    void OnConstants(const float* data);",
]
new_h1 = [
    "    /// A bound constant buffer of 864 bytes: checks the scene constant signature.",
    "    void OnConstants(const float* data);",
    "",
    "    /// v15: a bound 144-byte per-draw constant buffer of THIS build (CUSA01715 01.02):",
    "    /// floats [0..15] are the frame's view-projection matrix, column-major. Decomposes",
    "    /// it into view / inv_view / proj and feeds the camera (first feed of a frame wins).",
    "    void OnViewProj(const float* data);",
    "",
    "    /// v15: the scene render size, from the rasterizer's scene-color site (the 144-byte",
    "    /// buffer carries no size; OnConstants' 864-byte layout did).",
    "    void SetRenderSize(u32 width, u32 height);",
]

# --- C1: OnViewProj + SetRenderSize implementations ----------------------------
old_c1 = [
    "void CameraMotion::OnGBufferPass(VideoCore::ImageId depth) {",
]
new_c1 = [
    "void CameraMotion::OnViewProj(const float* data) {",
    "    if (frame_has_camera) {",
    "        return; // the first one of a frame is the main camera",
    "    }",
    "    // The stored matrix is column-major: logical row r is {data[r], data[4+r],",
    "    // data[8+r], data[12+r]}. For a perspective view-projection P * V with",
    "    // P = [f 0 0 0; 0 g 0 0; 0 0 A B; 0 0 -1 0]: row0 = f * Vrow0, row1 = g * Vrow1,",
    "    // row2 = A * Vrow2 (plus B in the w column), row3 = -Vrow2.",
    "    const float r0[4] = {data[0], data[4], data[8], data[12]};",
    "    const float r1[4] = {data[1], data[5], data[9], data[13]};",
    "    const float r2[4] = {data[2], data[6], data[10], data[14]};",
    "    const float r3[4] = {data[3], data[7], data[11], data[15]};",
    "    const float f = std::sqrt(r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]);",
    "    const float g = std::sqrt(r1[0] * r1[0] + r1[1] * r1[1] + r1[2] * r1[2]);",
    "    if (f < 0.05f || g < 0.05f) {",
    "        return; // not a view-projection (identity / post-pass constants)",
    "    }",
    "    std::array<float, 12> view{};",
    "    for (int c = 0; c < 4; ++c) {",
    "        view[c] = r0[c] / f;",
    "        view[4 + c] = r1[c] / g;",
    "    }",
    "    for (int c = 0; c < 4; ++c) {",
    "        view[8 + c] = -r3[c];",
    "    }",
    "    const float a = r2[0] * view[8] + r2[1] * view[9] + r2[2] * view[10];",
    "    const float b = r2[3] - a * view[11];",
    "    // inv_view: the rigid inverse (R transposed, -R^T t).",
    "    std::array<float, 12> inv{};",
    "    for (int r = 0; r < 3; ++r) {",
    "        for (int c = 0; c < 3; ++c) {",
    "            inv[r * 4 + c] = view[c * 4 + r];",
    "        }",
    "        inv[r * 4 + 3] = -(inv[r * 4 + 0] * view[3] + inv[r * 4 + 1] * view[7] +",
    "                           inv[r * 4 + 2] * view[11]);",
    "    }",
    "    previous = current;",
    "    current.view = view;",
    "    current.inv_view = inv;",
    "    current.proj = {f, g, a, b};",
    "    current.valid = true;",
    "    frame_has_camera = true;",
    "    static bool reported = false;",
    "    if (!reported) {",
    "        reported = true;",
    '        std::printf("Camera motion: view-proj feed active: f=%.3f g=%.3f A=%.4f B=%.4f, "',
    '                    "view row2 = %.3f %.3f %.3f %.1f\\n",',
    "                    f, g, a, b, view[8], view[9], view[10], view[11]);",
    "    }",
    "}",
    "",
    "void CameraMotion::SetRenderSize(u32 width, u32 height) {",
    "    const std::array<u32, 2> size{width, height};",
    "    if (size != render_size) {",
    '        std::printf("Camera motion: scene render size %ux%u (v15 feed)\\n", width, height);',
    "        render_size = size;",
    "    }",
    "}",
    "",
    "void CameraMotion::OnGBufferPass(VideoCore::ImageId depth) {",
]

# --- R1: VP feed at the vsharp site ---------------------------------------------
old_r1 = [
    "            if (vsharp.GetSize() == 864 && memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
]
new_r1 = [
    "            // v15: this build's camera feed - the 144-byte per-draw constant buffer",
    "            // carries the frame's view-projection at floats [0..15].",
    "            if (camera_motion->Enabled() && vsharp.GetSize() == 144 &&",
    "                memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
    "                camera_motion->OnViewProj(reinterpret_cast<const float*>(vsharp.base_address));",
    "            }",
    "            if (vsharp.GetSize() == 864 && memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
]

# --- R2: scene depth + size feed before the scene-color gate ---------------------
old_r2 = [
    "    // gow3: scene color: a full-size RGBA16F target drawn with the scene depth.",
    "    if (upscaler->Enabled() && db_desc.first && db_desc.first == camera_motion->Depth() &&",
]
new_r2 = [
    "    // v15: this build has no G-buffer pass (max 2 MRTs), so nothing else ever sets the",
    "    // camera depth. Take it from the first full-size scene draw: <=2 MRTs, an sRGB or",
    "    // RGBA16F color target, and color/depth sizes equal. OnGBufferPass just records it.",
    "    if (camera_motion->Enabled() && db_desc.first && !camera_motion->Depth() &&",
    "        cb_descs[0].first && std::popcount(key.mrt_mask) <= 2) {",
    "        const auto& feed_color = texture_cache.GetImage(cb_descs[0].first);",
    "        const auto& feed_depth = texture_cache.GetImage(db_desc.first);",
    "        if ((feed_color.info.pixel_format == vk::Format::eR8G8B8A8Srgb ||",
    "             feed_color.info.pixel_format == vk::Format::eR16G16B16A16Sfloat) &&",
    "            feed_color.info.size.width == feed_depth.info.size.width &&",
    "            feed_color.info.size.height == feed_depth.info.size.height) {",
    "            camera_motion->OnGBufferPass(db_desc.first);",
    "            camera_motion->SetRenderSize(feed_color.info.size.width,",
    "                                         feed_color.info.size.height);",
    "            static u32 feed_reports = 0;",
    "            if (++feed_reports <= 3 || feed_reports % 600 == 0) {",
    '                std::printf("Rasterizer: v15 scene depth feed: color %ux%u fmt=%d (report %u)\\n",',
    "                            feed_color.info.size.width, feed_color.info.size.height,",
    "                            static_cast<int>(feed_color.info.pixel_format), feed_reports);",
    "            }",
    "        }",
    "    }",
    "    // gow3: scene color: a full-size RGBA16F target drawn with the scene depth.",
    "    if (upscaler->Enabled() && db_desc.first && db_desc.first == camera_motion->Depth() &&",
]

# --- R3: scene-color gate accepts the sRGB scene target --------------------------
old_r3 = [
    "        if (color.info.pixel_format == vk::Format::eR16G16B16A16Sfloat &&",
    "            color.info.size.width == depth.info.size.width &&",
]
new_r3 = [
    "        // v15: this build's scene target is R8G8B8A8Srgb; accept it so OnSceneColor",
    "        // fires. Run() keeps its own RGBA16F requirement and reports its skip reason.",
    "        if ((color.info.pixel_format == vk::Format::eR16G16B16A16Sfloat ||",
    "             color.info.pixel_format == vk::Format::eR8G8B8A8Srgb) &&",
    "            color.info.size.width == depth.info.size.width &&",
]

# --- T1: order-based trigger fallback --------------------------------------------
old_t1 = [
    "    if (cs_hash != trigger_hash || done_this_frame || failed || Scaled()) {",
    "        return;",
    "    }",
]
new_t1 = [
    "    // v15: the shipped trigger hash (0x9a9cf8a9) belongs to another build and never",
    "    // occurs in this one. Unless GOW3_UPSCALE_BEFORE_CS pins a hash, also fire on the",
    "    // first compute dispatch that arrives once this frame's scene color exists and the",
    "    // camera is Ready - that dispatch is the game's composite/tonemap pass, the anchor",
    "    // the trigger hash was meant to name.",
    "    static const bool hash_only = std::getenv(\"GOW3_UPSCALE_BEFORE_CS\") != nullptr;",
    "    const bool fire = cs_hash == trigger_hash ||",
    "                      (!hash_only && scene_color && camera_motion.Ready());",
    "    if (!fire || done_this_frame || failed || Scaled()) {",
    "        return;",
    "    }",
]

apply(CMH, [("H1 declarations", old_h1, new_h1)])
apply(CMC, [("C1 OnViewProj + SetRenderSize", old_c1, new_c1)])
apply(RZ, [
    ("R1 VP feed at vsharp site", old_r1, new_r1),
    ("R2 scene depth feed", old_r2, new_r2),
    ("R3 scene gate accepts sRGB", old_r3, new_r3),
])
apply(TU, [("T1 order-based trigger", old_t1, new_t1)])
print("all v15 edits applied")
