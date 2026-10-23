#!/usr/bin/env python3
"""v22: per-gate dispatch counters (final trigger diagnostics).

State after v21: the trigger STILL never fired in a long, clean session
(his last_run-18.log: 10,844 pad reads, 143.7 FPS, no bridge/merge/evaluate
prints; the session ended in an unrelated shutdown crash - a null read in
D3DSCache.dll from the probe's exit path, with the upscaler never having
run). The frame lifecycle is now fully mapped:

  scene draws (OnSceneColor sets scene_color; v15 feed sets depth+camera)
  -> CS dispatches (OnDispatch: the ONLY window where fire can be true)
  -> display pass -> OnFrameStart (scene_color = {}, done_this_frame = false)

OnDispatch early-outs on !fire / done_this_frame / failed / Scaled(), and
unpinned the skip print is unreachable (fire already contains the ready
terms). Which gate stays shut during real gameplay is not observable from
the existing prints - the frame-1500 watchdog samples OnFrameStart-phase
state, which is exactly the wrong phase.

v22 counts every OnDispatch call against each gate and has the watchdog
report the counters at frame 1500 and every 15000 frames after:
  dispatches     - total CS dispatches seen
  with_scene     - dispatches while scene_color was set
  with_ready     - dispatches while CameraMotion::Ready() was true
  in_window      - dispatches with both (fire candidates)
  blocked_done / blocked_failed / blocked_scaled - fire true but vetoed
No behavior change.
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

old_w1 = [
    "bool g_diag_run_entered = false;",
]
new_w1 = [
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

old_w2 = [
    "    const bool fire = cs_hash == trigger_hash ||",
    "                      (!hash_only && scene_color && camera_motion.Ready());",
    "    if (!fire || done_this_frame || failed || Scaled()) {",
    "        return;",
    "    }",
]
new_w2 = [
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

old_w3 = [
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
new_w3 = [
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

apply(TU, [
    ("W1 counter declarations", old_w1, new_w1),
    ("W2 OnDispatch counters", old_w2, new_w2),
    ("W3 watchdog report", old_w3, new_w3),
])
print("all v22 edits applied")
