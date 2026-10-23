#!/usr/bin/env python3
"""v11 diagnostics for the gow3_pc upscaler frame path (vk_rasterizer.cpp only).

v10's verdict (his session log, 2,000,000 PrepareRenderState calls):
- The 864-byte scene-constants buffer with the expected signature does NOT
  exist in this build: ring path saw 0 buffers of that size, vsharp path saw
  2,691 with 0 signature passes (their [0] values run 364-445, [1] negative -
  some other globals buffer, not camera constants).
- Scene color is NEVER RGBA16F: 0 of 815,603 candidates. Dominant formats:
  43 = R8G8B8A8Srgb (622,748), 37 = R8G8B8A8Unorm (188,997), 100 = R32Sfloat
  (3,587), 83 = R16G16Sfloat (271).

So the camera constants live in a different-sized buffer and/or at different
offsets. v11 stays observation-only and hunts for them directly:
- a census of every constant-buffer size >= 128 bytes on both feed paths
  (ring copies and vsharp buffers), printed in the @200k/@2M summaries;
- a content scan of every buffer 256..4096 bytes (budget: first 3000 buffers
  per summary segment) for the distinctive patterns: far plane 3000.0 with
  1/3000 next to it (kind 1), a lone 3000.0 (kind 2), and render-size pairs
  1920x1080 / 1280x720 (kind 3). Each new (size, offset, kind) hit prints the
  offset plus 12 floats of context, so the real layout can be read off.
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

# --- W1: size census + pattern scanner (file scope, after the v10 probe) -----
old_w1 = J(
    '                    path == 0 ? "ring" : "vsharp", data[0], data[1], data[4], data[5]);',
    "    }",
    "}",
    "} // namespace",
)
new_w1 = J(
    '                    path == 0 ? "ring" : "vsharp", data[0], data[1], data[4], data[5]);',
    "    }",
    "}",
    "",
    "// v11: hunt for the real scene-constants buffer. v10 proved the 864-byte buffer",
    "// with the expected signature does not exist in this build (2,691 vsharp buffers",
    "// of that size, 0 signature passes; ring path none), and scene color is never",
    "// RGBA16F (0 of 815,603 candidates; mostly R8G8B8A8Srgb). So v11 censuses every",
    "// constant-buffer size and scans buffer contents for the distinctive far-plane",
    "// pair (3000.0 next to 1/3000), a lone 3000.0, and render-size pairs",
    "// (1920,1080)/(1280,720), printing the offset and surrounding floats on a hit.",
    "u32 g_diag11_sizes[16] = {};",
    "u32 g_diag11_size_hits[16] = {};",
    "u32 g_diag11_size_count = 0;",
    "u32 g_diag11_scanned = 0;",
    "u32 g_diag11_hit_keys[16][3] = {};",
    "u32 g_diag11_hit_count = 0;",
    "",
    "void Diag11Scan(const float* data, u32 size_bytes, int path) {",
    "    if (size_bytes >= 128) {",
    "        bool known = false;",
    "        for (u32 i = 0; i < g_diag11_size_count; ++i) {",
    "            if (g_diag11_sizes[i] == size_bytes) {",
    "                ++g_diag11_size_hits[i];",
    "                known = true;",
    "                break;",
    "            }",
    "        }",
    "        if (!known && g_diag11_size_count < 16) {",
    "            g_diag11_sizes[g_diag11_size_count] = size_bytes;",
    "            g_diag11_size_hits[g_diag11_size_count] = 1;",
    "            ++g_diag11_size_count;",
    "        }",
    "    }",
    "    if (size_bytes < 256 || size_bytes > 4096 || g_diag11_scanned >= 3000) {",
    "        return;",
    "    }",
    "    ++g_diag11_scanned;",
    "    const u32 n = size_bytes / 4;",
    "    for (u32 i = 0; i + 1 < n; ++i) {",
    "        int kind = 0;",
    "        if (data[i] == 3000.0f) {",
    "            float err = data[i + 1] * 3000.0f - 1.0f;",
    "            if (err < 0.0f) {",
    "                err = -err;",
    "            }",
    "            kind = err <= 0.01f ? 1 : 2;",
    "        } else if ((data[i] == 1920.0f && data[i + 1] == 1080.0f) ||",
    "                   (data[i] == 1280.0f && data[i + 1] == 720.0f)) {",
    "            kind = 3;",
    "        }",
    "        if (kind == 0) {",
    "            continue;",
    "        }",
    "        bool seen = false;",
    "        for (u32 h = 0; h < g_diag11_hit_count; ++h) {",
    "            if (g_diag11_hit_keys[h][0] == size_bytes && g_diag11_hit_keys[h][1] == i &&",
    "                g_diag11_hit_keys[h][2] == static_cast<u32>(kind)) {",
    "                seen = true;",
    "                break;",
    "            }",
    "        }",
    "        if (seen || g_diag11_hit_count >= 16) {",
    "            continue;",
    "        }",
    "        g_diag11_hit_keys[g_diag11_hit_count][0] = size_bytes;",
    "        g_diag11_hit_keys[g_diag11_hit_count][1] = i;",
    "        g_diag11_hit_keys[g_diag11_hit_count][2] = static_cast<u32>(kind);",
    "        ++g_diag11_hit_count;",
    "        const u32 base = i >= 2 ? i - 2 : 0;",
    '        std::printf("Rasterizer diag: constants pattern kind=%d on %s path: size=%u "',
    '                    "float-offset=%u ctx:",',
    '                    kind, path == 0 ? "ring" : "vsharp", size_bytes, i);',
    "        for (u32 k = base; k < base + 12 && k < n; ++k) {",
    '            std::printf(" %g", data[k]);',
    "        }",
    '        std::printf("\\n");',
    "    }",
    "}",
    "} // namespace",
)

# --- W2: scan ring copies -----------------------------------------------------
old_w2 = J(
    "            if (!desc.IsSpecial()) {",
    "                if (ring->size == 864 && memory->IsValidGpuMapping(ring->address, 0)) {",
)
new_w2 = J(
    "            Diag11Scan(reinterpret_cast<const float*>(constant_ring->Data(ring->offset)),",
    "                       static_cast<u32>(ring->size), 0);",
    "            if (!desc.IsSpecial()) {",
    "                if (ring->size == 864 && memory->IsValidGpuMapping(ring->address, 0)) {",
)

# --- W3: scan vsharp buffers --------------------------------------------------
old_w3 = J(
    "            if (vsharp.GetSize() == 864 && memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
    "                Diag10Probe864(reinterpret_cast<const float*>(vsharp.base_address), 1);",
)
new_w3 = J(
    "            if (vsharp.base_address != 0 && memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
    "                Diag11Scan(reinterpret_cast<const float*>(vsharp.base_address),",
    "                           static_cast<u32>(vsharp.GetSize()), 1);",
    "            }",
    "            if (vsharp.GetSize() == 864 && memory->IsValidGpuMapping(vsharp.base_address, 0)) {",
    "                Diag10Probe864(reinterpret_cast<const float*>(vsharp.base_address), 1);",
)

# --- W4: census + scan results in the summaries -------------------------------
old_w4 = J(
    "            for (u32 i = 0; i < g_diag10_fmt_count; ++i) {",
    '                std::printf("Rasterizer diag: scene candidate format=%d count=%u\\n",',
    "                            g_diag10_fmts[i], g_diag10_fmt_hits[i]);",
    "            }",
    "        }",
)
new_w4 = J(
    "            for (u32 i = 0; i < g_diag10_fmt_count; ++i) {",
    '                std::printf("Rasterizer diag: scene candidate format=%d count=%u\\n",',
    "                            g_diag10_fmts[i], g_diag10_fmt_hits[i]);",
    "            }",
    '            std::printf("Rasterizer diag: constants scan: scanned=%u pattern-hits=%u\\n",',
    "                        g_diag11_scanned, g_diag11_hit_count);",
    "            for (u32 i = 0; i < g_diag11_size_count; ++i) {",
    '                std::printf("Rasterizer diag: constant buffer size=%u count=%u\\n",',
    "                            g_diag11_sizes[i], g_diag11_size_hits[i]);",
    "            }",
    "            g_diag11_scanned = 0;",
    "        }",
)

apply(RZ, [
    ("W1 census + scanner", old_w1, new_w1),
    ("W2 ring scan", old_w2, new_w2),
    ("W3 vsharp scan", old_w3, new_w3),
    ("W4 summary census", old_w4, new_w4),
])
print("all v11 edits applied")
