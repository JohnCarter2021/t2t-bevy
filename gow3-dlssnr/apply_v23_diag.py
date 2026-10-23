#!/usr/bin/env python3
"""v23: move the upscale anchor to the display pass (OnFrameStart).

v22's counters settled the trigger question for good (his last_run-19.log,
a very long session): 396,896 compute dispatches, with_scene=2,
with_ready=152, in_window=0, blocked_*=0. In this build (CUSA01715 01.02)
NO compute dispatch ever lands inside the scene window (scene_color set +
camera Ready), so the author's composite-CS anchor structurally cannot
fire; the few historical fires were one-off transition frames. The game's
composite is a graphics pass, not a compute dispatch.

The anchor that exists every frame is the display pass: the rasterizer
calls CameraMotion::OnDisplayPass + TemporalUpscaler::OnFrameStart while
preparing the first draw that targets a display buffer, BEFORE that draw
is recorded - so a Run() fired inside OnFrameStart still rewrites the
completed scene color before the composite draw consumes it, in the same
submission stream. The author's choreography already fits this point:
OnFrameStart's `reset` decision reads dispatched_last_frame (set by Run)
for the NEXT frame, scene_color is cleared only later in the function,
and the render-size context is documented to survive the frame boundary.

v23 therefore:
  X1: in OnFrameStart, right before the reset decision, fire Run() when
      the completed frame has a scene color, the camera is Ready, the
      upscaler is active, and no anchor already ran for this frame.
  X2: remove the heuristic fire from OnDispatch (the pinned-hash fire
      stays for manual tests) so the two anchors can never double-run.
  X3: remove the v22 counters and restore the one-time watchdog print.
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

# ---- X1: the display anchor (insert before the reset decision) ----
old_x1 = [
    "    if (changed || !dispatched_last_frame) reset = true;",
]
new_x1 = [
    "    // v23: the display anchor. In this build no compute dispatch ever lands in",
    "    // the scene window (v22 counters: 396,896 dispatches, in_window=0), so the",
    "    // composite-CS anchor can never fire - the composite is a graphics pass.",
    "    // OnFrameStart runs while the first display-buffer draw is prepared, BEFORE",
    "    // it is recorded, so Run() here still rewrites the completed scene color",
    "    // before the composite consumes it. Run() consumes this frame's `reset`",
    "    // state and sets dispatched_last_frame, which the lines below then use",
    "    // for the NEXT frame's reset decision, exactly as the author choreographed.",
    "    if (active && scene_color && !done_this_frame && !failed && !Scaled() &&",
    "        camera_motion.Ready()) {",
    "        static bool anchor_reported = false;",
    "        if (!anchor_reported) {",
    "            anchor_reported = true;",
    '            std::printf("Upscaler: v23 display anchor: upscaling at frame start\\n");',
    "        }",
    "        Run();",
    "    }",
    "    if (changed || !dispatched_last_frame) reset = true;",
]

# ---- X2: OnDispatch - pinned hash only, counters out ----
old_x2 = [
    "    static const bool hash_only = std::getenv(\"GOW3_UPSCALE_BEFORE_CS\") != nullptr;",
    "    const bool fire = cs_hash == trigger_hash ||",
    "                      (!hash_only && scene_color && camera_motion.Ready());",
    "    // v22 diagnostics: count this dispatch against every gate.",
    "    ++g_diag_od_calls;",
    "    if (scene_color) {",
    "        ++g_diag_od_scene;",
    "    }",
    "    if (camera_motion.Ready()) {",
    "        ++g_diag_od_ready;",
    "    }",
    "    if (scene_color && camera_motion.Ready()) {",
    "        ++g_diag_od_window;",
    "    }",
    "    if (!fire || done_this_frame || failed || Scaled()) {",
    "        if (fire && done_this_frame) {",
    "            ++g_diag_od_block_done;",
    "        }",
    "        if (fire && failed) {",
    "            ++g_diag_od_block_failed;",
    "        }",
    "        if (fire && Scaled()) {",
    "            ++g_diag_od_block_scaled;",
    "        }",
    "        return;",
    "    }",
]
new_x2 = [
    "    // v23: the heuristic fire is gone - no dispatch ever lands in the scene",
    "    // window in this build (v22 counters), and the display anchor in",
    "    // OnFrameStart now owns the upscale. Only the pinned hash fires here.",
    "    const bool fire = cs_hash == trigger_hash;",
    "    if (!fire || done_this_frame || failed || Scaled()) {",
    "        return;",
    "    }",
]

# ---- X3a: counter declarations out ----
old_x3a = [
    "bool g_diag_run_entered = false;",
    "// v22: per-gate OnDispatch counters - a silent session still names its blocker.",
    "u64 g_diag_od_calls = 0;",
    "u64 g_diag_od_scene = 0;",
    "u64 g_diag_od_ready = 0;",
    "u64 g_diag_od_window = 0;",
    "u64 g_diag_od_block_done = 0;",
    "u64 g_diag_od_block_failed = 0;",
    "u64 g_diag_od_block_scaled = 0;",
]
new_x3a = [
    "bool g_diag_run_entered = false;",
]

# ---- X3b: watchdog back to one-time ----
old_x3b = [
    "        static u32 diag_frames = 0;",
    "        if (camera_motion.Ready()) {",
    "            g_diag_camera_seen = true;",
    "        }",
    "        ++diag_frames;",
    "        if (!g_diag_run_entered &&",
    "            (diag_frames == 1500 || (diag_frames % 15000 == 0))) {",
    '            std::printf("Upscaler: dispatch watch at frame %u: trigger_seen=%d "',
    '                        "scene_color_seen=%d camera_ready_seen=%d active=%d upscaler=%d | "',
    '                        "dispatches=%llu with_scene=%llu with_ready=%llu in_window=%llu "',
    '                        "blocked_done=%llu blocked_failed=%llu blocked_scaled=%llu\\n",',
    "                        diag_frames, g_diag_trigger_seen ? 1 : 0, g_diag_scene_seen ? 1 : 0,",
    "                        g_diag_camera_seen ? 1 : 0, Active() ? 1 : 0,",
    "                        Gow3Settings::Get().upscaler.load(),",
    "                        static_cast<unsigned long long>(g_diag_od_calls),",
    "                        static_cast<unsigned long long>(g_diag_od_scene),",
    "                        static_cast<unsigned long long>(g_diag_od_ready),",
    "                        static_cast<unsigned long long>(g_diag_od_window),",
    "                        static_cast<unsigned long long>(g_diag_od_block_done),",
    "                        static_cast<unsigned long long>(g_diag_od_block_failed),",
    "                        static_cast<unsigned long long>(g_diag_od_block_scaled));",
    "        }",
]
new_x3b = [
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
]

apply(TU, [
    ("X1 display anchor", old_x1, new_x1),
    ("X2 OnDispatch pinned only", old_x2, new_x2),
    ("X3a counter declarations out", old_x3a, new_x3a),
    ("X3b watchdog one-time", old_x3b, new_x3b),
])
print("all v23 edits applied")
