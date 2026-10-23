#!/usr/bin/env python3
"""v10 diagnostics for the gow3_pc upscaler frame path (vk_rasterizer.cpp only).

v9's verdict (his session log, 2,000,000 PrepareRenderState calls):
max_mrt=2, gbuffer=0, constants=0, scene_evals=814824 with scene_ok=0 and
fail_depth=814824. The G-buffer gate (>=5 MRTs) never opens in this game, so
the camera feed was never even attempted, and the trigger hash 0x9a9cf8a9
never occurs (only CS hashes in the whole session: dc647184f6178b9e,
ccdebd80f02a77aa, 40e31a6e74201e68).

v10 stays observation-only (no behaviour change) and answers the two
questions v9 could not:
1. Does the 864-byte scene-constants buffer exist on ordinary draws, and
   would its signature pass? Both OnConstants call sites are probed on ANY
   draw (the gbuffer_draw condition is kept for the real feed; the probe is
   a read-only replica of the signature test that never touches CameraMotion).
2. What color formats do scene-color candidates actually have? v9's gate
   replication checked depth first, so format was never reached; v10 counts
   every candidate's format, including how many are RGBA16F at all.
The @200k/@2M summaries gain the census results.
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

# --- V1: census counters + the read-only 864 signature probe -----------------
old_v1 = J(
    "u32 g_diag9_scene_fail_size = 0;",
    "} // namespace",
)
new_v1 = J(
    "u32 g_diag9_scene_fail_size = 0;",
    "",
    "// v10: 864-byte scene-constants census. The camera feed (OnConstants) is only ever",
    "// attempted on G-buffer draws, which this game never issues (v9: max_mrt=2 over 2M",
    "// draws), so v10 probes every 864-byte constant buffer on ANY draw, read-only: it",
    "// runs the same signature test OnConstants would run, without feeding CameraMotion.",
    "u32 g_diag10_864_seen[2] = {};",
    "u32 g_diag10_864_pass[2] = {};",
    "float g_diag10_tuples[8][4] = {};",
    "u32 g_diag10_tuple_count = 0;",
    "int g_diag10_fmts[12] = {};",
    "u32 g_diag10_fmt_hits[12] = {};",
    "u32 g_diag10_fmt_count = 0;",
    "u32 g_diag10_rgba16f = 0;",
    "",
    "void Diag10Probe864(const float* data, int path) {",
    "    ++g_diag10_864_seen[path];",
    "    float prod_err = data[1] * data[0] - 1.0f;",
    "    if (prod_err < 0.0f) {",
    "        prod_err = -prod_err;",
    "    }",
    "    const bool pass = data[0] == 3000.0f && data[4] >= 64.0f && data[5] >= 64.0f &&",
    "                      prod_err <= 1e-3f;",
    "    if (pass) {",
    "        if (g_diag10_864_pass[path]++ == 0) {",
    '            std::printf("Rasterizer diag: 864-byte scene constants FOUND on %s path: "',
    '                        "[0]=%.3f [1]=%.6f [4]=%.1f [5]=%.1f\\n",',
    '                        path == 0 ? "ring" : "vsharp", data[0], data[1], data[4], data[5]);',
    "        }",
    "        return;",
    "    }",
    "    for (u32 i = 0; i < g_diag10_tuple_count; ++i) {",
    "        if (g_diag10_tuples[i][0] == data[0] && g_diag10_tuples[i][1] == data[1] &&",
    "            g_diag10_tuples[i][2] == data[4] && g_diag10_tuples[i][3] == data[5]) {",
    "            return;",
    "        }",
    "    }",
    "    if (g_diag10_tuple_count < 8) {",
    "        g_diag10_tuples[g_diag10_tuple_count][0] = data[0];",
    "        g_diag10_tuples[g_diag10_tuple_count][1] = data[1];",
    "        g_diag10_tuples[g_diag10_tuple_count][2] = data[4];",
    "        g_diag10_tuples[g_diag10_tuple_count][3] = data[5];",
    "        ++g_diag10_tuple_count;",
    '        std::printf("Rasterizer diag: 864 candidate on %s path rejected: [0]=%.3f "',
    '                    "[1]=%.6f [4]=%.1f [5]=%.1f\\n",',
    '                    path == 0 ? "ring" : "vsharp", data[0], data[1], data[4], data[5]);',
    "    }",
    "}",
    "} // namespace",
)

# --- V2: probe the ring-path 864 site on any draw ----------------------------
old_v2 = J(
    "                if (ring->size == 864 && gbuffer_draw &&",
    "                    memory->IsValidGpuMapping(ring->address, 0)) {",
    "                    if (g_diag9_constants++ == 0) {",
    '                        std::printf("Rasterizer diag: first OnConstants call (ring path)\\n");',
    "                    }",
    "                    camera_motion->OnConstants(reinterpret_cast<const float*>(ring->address));",
    "                }",
)
new_v2 = J(
    "                if (ring->size == 864 && memory->IsValidGpuMapping(ring->address, 0)) {",
    "                    Diag10Probe864(reinterpret_cast<const float*>(ring->address), 0);",
    "                    if (gbuffer_draw) {",
    "                        if (g_diag9_constants++ == 0) {",
    '                            std::printf("Rasterizer diag: first OnConstants call (ring path)\\n");',
    "                        }",
    "                        camera_motion->OnConstants(reinterpret_cast<const float*>(ring->address));",
    "                    }",
    "                }",
)

# --- V3: probe the vsharp-path 864 site on any draw --------------------------
old_v3 = J(
    "            if (vsharp.GetSize() == 864 && gbuffer_draw &&",
    "                memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
    "                if (g_diag9_constants++ == 0) {",
    '                    std::printf("Rasterizer diag: first OnConstants call (vsharp path)\\n");',
    "                }",
    "                camera_motion->OnConstants(reinterpret_cast<const float*>(vsharp.base_address));",
    "            }",
)
new_v3 = J(
    "            if (vsharp.GetSize() == 864 && memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
    "                Diag10Probe864(reinterpret_cast<const float*>(vsharp.base_address), 1);",
    "                if (gbuffer_draw) {",
    "                    if (g_diag9_constants++ == 0) {",
    '                        std::printf("Rasterizer diag: first OnConstants call (vsharp path)\\n");',
    "                    }",
    "                    camera_motion->OnConstants(reinterpret_cast<const float*>(vsharp.base_address));",
    "                }",
    "            }",
)

# --- V4: scene-candidate format census (before the depth check) --------------
old_v4 = J(
    "    if (upscaler->Enabled() && db_desc.first && cb_descs[0].first) {",
    "        ++g_diag9_scene_evals;",
    "        if (db_desc.first != camera_motion->Depth()) {",
)
new_v4 = J(
    "    if (upscaler->Enabled() && db_desc.first && cb_descs[0].first) {",
    "        ++g_diag9_scene_evals;",
    "        {",
    "            const int fmt =",
    "                static_cast<int>(texture_cache.GetImage(cb_descs[0].first).info.pixel_format);",
    "            if (fmt == static_cast<int>(vk::Format::eR16G16B16A16Sfloat)) {",
    "                ++g_diag10_rgba16f;",
    "            }",
    "            bool fmt_known = false;",
    "            for (u32 i = 0; i < g_diag10_fmt_count; ++i) {",
    "                if (g_diag10_fmts[i] == fmt) {",
    "                    ++g_diag10_fmt_hits[i];",
    "                    fmt_known = true;",
    "                    break;",
    "                }",
    "            }",
    "            if (!fmt_known && g_diag10_fmt_count < 12) {",
    "                g_diag10_fmts[g_diag10_fmt_count] = fmt;",
    "                g_diag10_fmt_hits[g_diag10_fmt_count] = 1;",
    "                ++g_diag10_fmt_count;",
    "            }",
    "        }",
    "        if (db_desc.first != camera_motion->Depth()) {",
)

# --- V5: census results in the @200k/@2M summaries ---------------------------
old_v5 = J(
    "                        g_diag9_scene_fail_mrt, g_diag9_scene_fail_format, g_diag9_scene_fail_size);",
    "        }",
)
new_v5 = J(
    "                        g_diag9_scene_fail_mrt, g_diag9_scene_fail_format, g_diag9_scene_fail_size);",
    '            std::printf("Rasterizer diag: 864 census: ring seen=%u sig-pass=%u, vsharp "',
    '                        "seen=%u sig-pass=%u, distinct rejected tuples=%u; scene candidates "',
    '                        "RGBA16F=%u\\n",',
    "                        g_diag10_864_seen[0], g_diag10_864_pass[0], g_diag10_864_seen[1],",
    "                        g_diag10_864_pass[1], g_diag10_tuple_count, g_diag10_rgba16f);",
    "            for (u32 i = 0; i < g_diag10_fmt_count; ++i) {",
    '                std::printf("Rasterizer diag: scene candidate format=%d count=%u\\n",',
    "                            g_diag10_fmts[i], g_diag10_fmt_hits[i]);",
    "            }",
    "        }",
)

apply(RZ, [
    ("V1 census + probe", old_v1, new_v1),
    ("V2 ring probe", old_v2, new_v2),
    ("V3 vsharp probe", old_v3, new_v3),
    ("V4 format census", old_v4, new_v4),
    ("V5 summary census", old_v5, new_v5),
])
print("all v10 edits applied")
