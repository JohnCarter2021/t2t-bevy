#!/usr/bin/env python3
"""v21: stop OnDisplayPass from clearing the scene depth.

Pin test 2 (his last_run-17.log, trigger pinned to 40e31a6e74201e68):
"Upscaler: trigger matched but the frame was skipped: scene_color=1
camera_ready=0 active=1" - mid-gameplay, repeatedly (the print is
one-time). The composite-class dispatch and the scene color are both
present; CameraMotion::Ready() = current.valid && previous.valid &&
depth_id fails on the depth term alone.

Cause (source): CameraMotion::OnDisplayPass ends with `depth_id = {}`.
In this build's frame order the display pass is recorded before the
composite dispatch, so at every trigger check the depth has just been
cleared - while the v15 scene feed only re-sets it at the next frame's
first full-size scene draw. The author's design set the depth from a
G-buffer pass that precedes everything; this build has no such pass.

The depth target does not change between frames unless the scene
re-renders, and the v15 feed re-sets depth_id at the first scene draw of
every frame regardless - so keeping the last depth across the display
pass is the correct semantics for the upscaler (and the overlay, the
only other consumer, reads depth_id before the clear anyway).
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

CM = "gpu/shadps4/video_core/renderer_vulkan/vk_camera_motion.cpp"

old_c1 = [
    "    if (!frame_has_camera) InvalidateHistory();",
    "    frame_has_camera = false;",
    "    depth_id = {};",
    "}",
]
new_c1 = [
    "    if (!frame_has_camera) InvalidateHistory();",
    "    frame_has_camera = false;",
    "    // v21: keep the scene depth across the display pass. In this build the",
    "    // display pass is recorded BEFORE the composite dispatch, so clearing",
    "    // depth_id here made CameraMotion::Ready() false at every upscaler",
    "    // trigger check (pin test: scene_color=1 camera_ready=0). The v15 feed",
    "    // re-sets the depth at the first scene draw of every frame anyway.",
    "}",
]

apply(CM, [("C1 keep depth across display pass", old_c1, new_c1)])
print("all v21 edits applied")
