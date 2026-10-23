#!/usr/bin/env python3
"""v9 gate diagnostics for the gow3_pc upscaler frame path.

v8 proved the launcher gate is lifted (constructor line prints, Active=1,
upscaler=5) yet Run() still never fires. v7's summary showed all three
remaining gates shut at frame 1500: trigger_seen=0, scene_color_seen=0,
camera_ready_seen=0. The scene-color and camera gates both hang off
gbuffer_draw in vk_rasterizer.cpp (popcount(mrt_mask) >= 5 on a draw with
depth), which the upscaler-side instrumentation could not observe.

v9 therefore counts at the rasterizer call sites (read-only replications of
the gate tests; no behaviour changes):
- PrepareRenderState calls, max MRT popcount, gbuffer_draw hits,
  OnConstants invocations (ring + vsharp paths),
- scene-color gate outcomes: ok / depth-mismatch / mrt / format / size,
with one-time prints for the first G-buffer draw, first constants call and
first wrong-format scene candidate, plus counter summaries at 200k and 2M
PrepareRenderState calls.
And in vk_temporal_upscaler.cpp, OnDispatch now names every distinct CS
hash (cap raised 24 -> 32) instead of only post-scene-color ones, so the
real trigger-hash candidates are visible even while scene_color is unset.
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

RZ = "gpu/shadps4/video_core/renderer_vulkan/vk_rasterizer.cpp"
UP = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.cpp"

# --- R1: diag counters -------------------------------------------------------
old_r1 = J(
    "namespace Vulkan {",
    "",
    "static Shader::PushData MakeUserData(const AmdGpu::Regs& regs) {",
)
new_r1 = J(
    "namespace Vulkan {",
    "",
    "namespace {",
    "// v9 gate diagnostics: counters only. They name which of the upscaler's frame-path",
    "// gates (G-buffer draw, camera constants, scene color) actually opens in a session.",
    "u32 g_diag9_draws = 0;",
    "u32 g_diag9_max_mrt = 0;",
    "u32 g_diag9_gbuffer = 0;",
    "u32 g_diag9_constants = 0;",
    "u32 g_diag9_scene_evals = 0;",
    "u32 g_diag9_scene_ok = 0;",
    "u32 g_diag9_scene_fail_depth = 0;",
    "u32 g_diag9_scene_fail_mrt = 0;",
    "u32 g_diag9_scene_fail_format = 0;",
    "u32 g_diag9_scene_fail_size = 0;",
    "} // namespace",
    "",
    "static Shader::PushData MakeUserData(const AmdGpu::Regs& regs) {",
)

# --- R2: G-buffer draw counting in PrepareRenderState ------------------------
old_r2 = J(
    "    gbuffer_draw = camera_motion->Enabled() && std::popcount(key.mrt_mask) >= 5 && db_desc.first;",
    "    if (gbuffer_draw) {",
    "        camera_motion->OnGBufferPass(db_desc.first);",
    "    }",
)
new_r2 = J(
    "    gbuffer_draw = camera_motion->Enabled() && std::popcount(key.mrt_mask) >= 5 && db_desc.first;",
    "    if (gbuffer_draw) {",
    "        camera_motion->OnGBufferPass(db_desc.first);",
    "    }",
    "    {",
    "        ++g_diag9_draws;",
    "        const u32 mrt_count = std::popcount(key.mrt_mask);",
    "        if (mrt_count > g_diag9_max_mrt) {",
    "            g_diag9_max_mrt = mrt_count;",
    "        }",
    "        if (gbuffer_draw) {",
    "            if (g_diag9_gbuffer++ == 0) {",
    '                std::printf("Rasterizer diag: first G-buffer draw: mrt=%u\\n", mrt_count);',
    "            }",
    "        } else if (camera_motion->Enabled() && db_desc.first && mrt_count >= 4) {",
    "            static bool near_reported = false;",
    "            if (!near_reported) {",
    "                near_reported = true;",
    '                std::printf("Rasterizer diag: near-miss draw: mrt=%u (G-buffer needs >= 5) with depth bound\\n",',
    "                            mrt_count);",
    "            }",
    "        }",
    "        if (g_diag9_draws == 200000 || g_diag9_draws == 2000000) {",
    '            std::printf("Rasterizer diag @%u: max_mrt=%u gbuffer=%u constants=%u scene_evals=%u "',
    '                        "scene_ok=%u fail_depth=%u fail_mrt=%u fail_format=%u fail_size=%u\\n",',
    "                        g_diag9_draws, g_diag9_max_mrt, g_diag9_gbuffer, g_diag9_constants,",
    "                        g_diag9_scene_evals, g_diag9_scene_ok, g_diag9_scene_fail_depth,",
    "                        g_diag9_scene_fail_mrt, g_diag9_scene_fail_format, g_diag9_scene_fail_size);",
    "        }",
    "    }",
)

# --- R3: scene-color gate outcome counting (read-only replication) ---------
old_r3 = J(
    "    if (upscaler->Enabled() && cb_descs[0].first) {",
    "        upscaler->OnColorTarget(cb_descs[0].first);",
    "    }",
)
new_r3 = J(
    "    if (upscaler->Enabled() && cb_descs[0].first) {",
    "        upscaler->OnColorTarget(cb_descs[0].first);",
    "    }",
    "    if (upscaler->Enabled() && db_desc.first && cb_descs[0].first) {",
    "        ++g_diag9_scene_evals;",
    "        if (db_desc.first != camera_motion->Depth()) {",
    "            ++g_diag9_scene_fail_depth;",
    "        } else if (std::popcount(key.mrt_mask) > 2) {",
    "            ++g_diag9_scene_fail_mrt;",
    "        } else {",
    "            const auto& diag_color = texture_cache.GetImage(cb_descs[0].first);",
    "            const auto& diag_depth = texture_cache.GetImage(db_desc.first);",
    "            if (diag_color.info.pixel_format != vk::Format::eR16G16B16A16Sfloat) {",
    "                if (g_diag9_scene_fail_format++ == 0) {",
    '                    std::printf("Rasterizer diag: scene candidate color format=%d, not RGBA16F\\n",',
    "                                static_cast<int>(diag_color.info.pixel_format));",
    "                }",
    "            } else if (diag_color.info.size.width != diag_depth.info.size.width ||",
    "                       diag_color.info.size.height != diag_depth.info.size.height) {",
    "                ++g_diag9_scene_fail_size;",
    "            } else {",
    "                ++g_diag9_scene_ok;",
    "            }",
    "        }",
    "    }",
)

# --- R4: OnConstants ring-path counting --------------------------------------
old_r4 = J(
    "                if (ring->size == 864 && gbuffer_draw &&",
    "                    memory->IsValidGpuMapping(ring->address, 0)) {",
    "                    camera_motion->OnConstants(reinterpret_cast<const float*>(ring->address));",
    "                }",
)
new_r4 = J(
    "                if (ring->size == 864 && gbuffer_draw &&",
    "                    memory->IsValidGpuMapping(ring->address, 0)) {",
    "                    if (g_diag9_constants++ == 0) {",
    '                        std::printf("Rasterizer diag: first OnConstants call (ring path)\\n");',
    "                    }",
    "                    camera_motion->OnConstants(reinterpret_cast<const float*>(ring->address));",
    "                }",
)

# --- R5: OnConstants vsharp-path counting ------------------------------------
old_r5 = J(
    "            if (vsharp.GetSize() == 864 && gbuffer_draw &&",
    "                memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
    "                camera_motion->OnConstants(reinterpret_cast<const float*>(vsharp.base_address));",
    "            }",
)
new_r5 = J(
    "            if (vsharp.GetSize() == 864 && gbuffer_draw &&",
    "                memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
    "                if (g_diag9_constants++ == 0) {",
    '                    std::printf("Rasterizer diag: first OnConstants call (vsharp path)\\n");',
    "                }",
    "                camera_motion->OnConstants(reinterpret_cast<const float*>(vsharp.base_address));",
    "            }",
)

# --- U1..U4: OnDispatch names every distinct CS hash --------------------------
old_u1 = "u64 g_diag_late_hashes[24]{};"
new_u1 = "u64 g_diag_late_hashes[32]{};"

old_u2 = J(
    "    } else if (scene_color) {",
    "        // A compute dispatch after the scene color exists belongs to the game's post",
    "        // chain: the trigger hash is one of these, so name each new one once.",
)
new_u2 = J(
    "    } else {",
    "        // Name every distinct compute hash once: the trigger is one of the game's real",
    "        // dispatches, and this list is how we find it even before scene_color exists.",
)

old_u3 = "        if (!known && g_diag_late_count < 24) {"
new_u3 = "        if (!known && g_diag_late_count < 32) {"

old_u4 = '            std::printf("Upscaler: late-frame CS %016llx seen (trigger is %016llx)\\n",'
new_u4 = '            std::printf("Upscaler: dispatch CS %016llx seen (trigger is %016llx)\\n",'

apply(RZ, [
    ("R1 counters", old_r1, new_r1),
    ("R2 gbuffer counting", old_r2, new_r2),
    ("R3 scene gate outcomes", old_r3, new_r3),
    ("R4 constants ring", old_r4, new_r4),
    ("R5 constants vsharp", old_r5, new_r5),
])
apply(UP, [
    ("U1 hash table size", old_u1, new_u1),
    ("U2 unconditional hashes", old_u2, new_u2),
    ("U3 hash cap", old_u3, new_u3),
    ("U4 hash print text", old_u4, new_u4),
])
print("all v9 edits applied")
