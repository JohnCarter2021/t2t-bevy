#!/usr/bin/env python3
"""v7 one-time gate diagnostics for the gow3_pc upscaler frame path.

Adds printf-once instrumentation only; no behaviour changes. Each edit is
anchored on exact current text and must match exactly once.
"""
import sys

CRLF = "\r\n"

def J(*lines):
    return CRLF.join(lines)

def apply(path, edits):
    with open(path, "r", newline="") as f:
        content = f.read()
    for name, old, new in edits:
        n = content.count(old)
        if n != 1:
            print(f"FAIL {path} :: {name}: anchor found {n} times")
            sys.exit(1)
        content = content.replace(old, new)
        print(f"ok   {path} :: {name}")
    with open(path, "w", newline="") as f:
        f.write(content)

UP = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.cpp"
CM = "gpu/shadps4/video_core/renderer_vulkan/vk_camera_motion.cpp"

# --- A: diag state before the TemporalUpscaler constructor -------------------
old_a = J(
    "} // namespace",
    "",
    "TemporalUpscaler::TemporalUpscaler(const Instance& instance_, Scheduler& scheduler_,",
)
new_a = J(
    "} // namespace",
    "",
    "namespace {",
    "// One-time diagnostics for a session where the upscaler never dispatches: they name",
    "// the gate that stayed shut instead of failing silently.",
    "bool g_diag_trigger_seen = false;",
    "bool g_diag_scene_seen = false;",
    "bool g_diag_camera_seen = false;",
    "bool g_diag_run_entered = false;",
    "u64 g_diag_late_hashes[24]{};",
    "u32 g_diag_late_count = 0;",
    "} // namespace",
    "",
    "TemporalUpscaler::TemporalUpscaler(const Instance& instance_, Scheduler& scheduler_,",
)

# --- B: OnSceneColor marks that a scene color was seen -----------------------
old_b = J(
    "void TemporalUpscaler::OnSceneColor(VideoCore::ImageId color) {",
    "    scene_color = color;",
    "}",
)
new_b = J(
    "void TemporalUpscaler::OnSceneColor(VideoCore::ImageId color) {",
    "    g_diag_scene_seen = true;",
    "    scene_color = color;",
    "}",
)

# --- C: OnDispatch: trigger sighting, late-CS candidates, gate report -------
old_c = J(
    "void TemporalUpscaler::OnDispatch(u64 cs_hash) {",
    "    if (cs_hash != trigger_hash || done_this_frame || failed || Scaled()) {",
    "        return;",
    "    }",
    "    done_this_frame = true;",
    "    if (!scene_color || !camera_motion.Ready() || !Active()) {",
    "        reset = true;",
    "        return;",
    "    }",
    "    Run();",
    "}",
)
new_c = J(
    "void TemporalUpscaler::OnDispatch(u64 cs_hash) {",
    "    if (cs_hash == trigger_hash) {",
    "        g_diag_trigger_seen = true;",
    "    } else if (scene_color) {",
    "        // A compute dispatch after the scene color exists belongs to the game's post",
    "        // chain: the trigger hash is one of these, so name each new one once.",
    "        bool known = false;",
    "        for (u32 i = 0; i < g_diag_late_count; ++i) {",
    "            known = known || g_diag_late_hashes[i] == cs_hash;",
    "        }",
    "        if (!known && g_diag_late_count < 24) {",
    "            g_diag_late_hashes[g_diag_late_count++] = cs_hash;",
    '            std::printf("Upscaler: late-frame CS %016llx seen (trigger is %016llx)\\n",',
    "                        static_cast<unsigned long long>(cs_hash),",
    "                        static_cast<unsigned long long>(trigger_hash));",
    "        }",
    "    }",
    "    if (cs_hash != trigger_hash || done_this_frame || failed || Scaled()) {",
    "        return;",
    "    }",
    "    done_this_frame = true;",
    "    if (!scene_color || !camera_motion.Ready() || !Active()) {",
    "        static bool reported = false;",
    "        if (!reported) {",
    "            reported = true;",
    '            std::printf("Upscaler: trigger matched but the frame was skipped: "',
    '                        "scene_color=%d camera_ready=%d active=%d\\n",',
    "                        scene_color ? 1 : 0, camera_motion.Ready() ? 1 : 0, Active() ? 1 : 0);",
    "        }",
    "        reset = true;",
    "        return;",
    "    }",
    "    Run();",
    "}",
)

# --- D: OnFrameStart: 1500-frame no-dispatch summary -------------------------
old_d = J(
    "    const auto& settings = Gow3Settings::Get();",
    "    // The guest's startup resolution patch remains in effect until restart. Keep the FSR",
)
new_d = J(
    "    {",
    "        static u32 diag_frames = 0;",
    "        static bool diag_reported = false;",
    "        if (camera_motion.Ready()) {",
    "            g_diag_camera_seen = true;",
    "        }",
    "        if (!diag_reported && ++diag_frames == 1500 && !g_diag_run_entered) {",
    "            diag_reported = true;",
    '            std::printf("Upscaler: still no dispatch after 1500 frames: trigger_seen=%d "',
    '                        "scene_color_seen=%d camera_ready_seen=%d active=%d upscaler=%d\\n",',
    "                        g_diag_trigger_seen ? 1 : 0, g_diag_scene_seen ? 1 : 0,",
    "                        g_diag_camera_seen ? 1 : 0, Active() ? 1 : 0,",
    "                        Gow3Settings::Get().upscaler.load());",
    "        }",
    "    }",
    "    const auto& settings = Gow3Settings::Get();",
    "    // The guest's startup resolution patch remains in effect until restart. Keep the FSR",
)

# --- E1: Run() entry flag ------------------------------------------------------
old_e1 = J(
    "void TemporalUpscaler::Run() {",
    "    if (auto* profiler = GpuProfiler::Get()) {",
)
new_e1 = J(
    "void TemporalUpscaler::Run() {",
    "    g_diag_run_entered = true;",
    "    if (auto* profiler = GpuProfiler::Get()) {",
)

# --- E2: Run() silent early-out gets a one-time print --------------------------
old_e2 = J(
    "    if (color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat ||",
    "        depth.info.size.width != ow || depth.info.size.height != oh ||",
    "        !(color.usage_flags & vk::ImageUsageFlagBits::eStorage)) {",
    "        return;",
    "    }",
)
new_e2 = J(
    "    if (color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat ||",
    "        depth.info.size.width != ow || depth.info.size.height != oh ||",
    "        !(color.usage_flags & vk::ImageUsageFlagBits::eStorage)) {",
    "        static bool reported = false;",
    "        if (!reported) {",
    "            reported = true;",
    '            std::printf("Upscaler: Run skipped a frame: color %ux%u, depth %ux%u, color "',
    '                        "storage usage=%d, color format=%d\\n",',
    "                        ow, oh, depth.info.size.width, depth.info.size.height,",
    "                        (color.usage_flags & vk::ImageUsageFlagBits::eStorage) ? 1 : 0,",
    "                        static_cast<int>(color.info.pixel_format));",
    "        }",
    "        return;",
    "    }",
)

# --- F: CameraMotion::OnConstants signature reject print ----------------------
old_f = J(
    "    if (data[0] != 3000.0f || data[4] < 64.0f || data[5] < 64.0f ||",
    "        std::abs(data[1] * data[0] - 1.0f) > 1e-3f) {",
    "        return;",
    "    }",
)
new_f = J(
    "    if (data[0] != 3000.0f || data[4] < 64.0f || data[5] < 64.0f ||",
    "        std::abs(data[1] * data[0] - 1.0f) > 1e-3f) {",
    "        static bool reported = false;",
    "        if (!reported) {",
    "            reported = true;",
    '            std::printf("Camera motion: scene constants rejected: [0]=%.3f [1]=%.6f "',
    '                        "[4]=%.1f [5]=%.1f\\n",',
    "                        data[0], data[1], data[4], data[5]);",
    "        }",
    "        return;",
    "    }",
)

apply(UP, [
    ("A diag state", old_a, new_a),
    ("B OnSceneColor", old_b, new_b),
    ("C OnDispatch", old_c, new_c),
    ("D OnFrameStart summary", old_d, new_d),
    ("E1 Run flag", old_e1, new_e1),
    ("E2 Run early-out", old_e2, new_e2),
])
apply(CM, [
    ("F OnConstants reject", old_f, new_f),
])
print("all edits applied")
