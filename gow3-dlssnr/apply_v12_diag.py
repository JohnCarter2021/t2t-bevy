#!/usr/bin/env python3
"""v12 diagnostics for the gow3_pc upscaler camera hunt (vk_rasterizer.cpp only).

v11's verdict (his verified session log, 2,000,000 draws): the content scan
found NOTHING - scanned=3000 per segment, pattern-hits=0. No 3000.0, no
far+1/far pair, no render-size pair in any scanned buffer, and v10 already
proved the 864-byte buffers do not hold them either. The far value 3000
itself is probably wrong for CUSA01715 01.02 (it came from the WIP author's
test build). v11's census also exposed two suspects it never scanned: a
16,256-byte buffer bound ~703k times per 2M draws (scan cap was 4,096) and
small 144/160-byte buffers bound ~1.1M times (scan floor was 256).

v12 therefore tests STRUCTURE, not one game's constants:
- scan range widened to 128..32,768 bytes;
- buffers are deduped by identity + a hash of their first 64 bytes, so a
  dynamic buffer rescans when its head changes (budget 20,000 scans per
  summary segment);
- detectors: kind 1 = a value in [250, 100000] with a reciprocal neighbour
  (far + 1/far for ANY far value), kind 3 = integer render-size pairs,
  kind 4 = a 3x4 view-like matrix (two unit rows, orthogonal, third row
  equal to their cross product), kind 5 = a 4x4 projection-like matrix
  (sparse perspective pattern). Hits print size, float offset, 16 floats.
Call sites move from Diag11Scan to Diag12Scan; all v9/v10/v11 diagnostics
stay in place.
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

# --- X1: v12 census + structure scanner (after Diag11Scan, same namespace) ---
old_x1 = J(
    '        std::printf("\\n");',
    "    }",
    "}",
    "} // namespace",
)
new_x1 = J(
    '        std::printf("\\n");',
    "    }",
    "}",
    "",
    "// v12: wider, structure-based camera hunt. v11 scanned only 256..4096-byte",
    "// buffers for the literal far pair and found nothing (pattern-hits=0), and its",
    "// census showed the most-bound sizes were never scanned at all: a 16,256-byte",
    "// buffer bound ~700k times per segment and small 144/160-byte buffers bound",
    "// ~1M times. v12 scans 128..32768 bytes, dedupes by buffer identity + a hash",
    "// of the first 64 bytes (dynamic buffers rescan when their head changes), and",
    "// tests structure: kind 1 = value with reciprocal neighbour (far + 1/far, any",
    "// far value), kind 3 = integer render-size pair, kind 4 = 3x4 view-like",
    "// matrix, kind 5 = 4x4 projection-like matrix.",
    "u32 g_diag12_sizes[24] = {};",
    "u32 g_diag12_size_hits[24] = {};",
    "u32 g_diag12_size_count = 0;",
    "u32 g_diag12_scanned = 0;",
    "u64 g_diag12_seen_key[512] = {};",
    "u32 g_diag12_seen_size[512] = {};",
    "u32 g_diag12_seen_head[512] = {};",
    "u32 g_diag12_seen_count = 0;",
    "u32 g_diag12_hit_keys[24][3] = {};",
    "u32 g_diag12_hit_count = 0;",
    "",
    "float Diag12Abs(float x) {",
    "    return x < 0.0f ? -x : x;",
    "}",
    "",
    "u32 Diag12HeadHash(const float* data, u32 size_bytes) {",
    "    u32 words = size_bytes / 4;",
    "    if (words > 16) {",
    "        words = 16;",
    "    }",
    "    u32 h = 2166136261u;",
    "    for (u32 i = 0; i < words; ++i) {",
    "        u32 bits = 0;",
    "        std::memcpy(&bits, data + i, sizeof(bits));",
    "        h = (h ^ bits) * 16777619u;",
    "    }",
    "    return h;",
    "}",
    "",
    "void Diag12Report(int kind, u32 size_bytes, u32 off, const float* data, u32 n, int path) {",
    "    for (u32 h = 0; h < g_diag12_hit_count; ++h) {",
    "        if (g_diag12_hit_keys[h][0] == size_bytes && g_diag12_hit_keys[h][1] == off &&",
    "            g_diag12_hit_keys[h][2] == static_cast<u32>(kind)) {",
    "            return;",
    "        }",
    "    }",
    "    if (g_diag12_hit_count >= 24) {",
    "        return;",
    "    }",
    "    g_diag12_hit_keys[g_diag12_hit_count][0] = size_bytes;",
    "    g_diag12_hit_keys[g_diag12_hit_count][1] = off;",
    "    g_diag12_hit_keys[g_diag12_hit_count][2] = static_cast<u32>(kind);",
    "    ++g_diag12_hit_count;",
    '    std::printf("Rasterizer diag: v12 hit kind=%d on %s path: size=%u float-offset=%u ctx:",',
    '                kind, path == 0 ? "ring" : "vsharp", size_bytes, off);',
    "    const u32 end = off + 16 < n ? off + 16 : n;",
    "    for (u32 k = off; k < end; ++k) {",
    '        std::printf(" %g", data[k]);',
    "    }",
    '    std::printf("\\n");',
    "}",
    "",
    "void Diag12Scan(const float* data, u32 size_bytes, u64 key, int path) {",
    "    if (size_bytes >= 64) {",
    "        bool known = false;",
    "        for (u32 i = 0; i < g_diag12_size_count; ++i) {",
    "            if (g_diag12_sizes[i] == size_bytes) {",
    "                ++g_diag12_size_hits[i];",
    "                known = true;",
    "                break;",
    "            }",
    "        }",
    "        if (!known && g_diag12_size_count < 24) {",
    "            g_diag12_sizes[g_diag12_size_count] = size_bytes;",
    "            g_diag12_size_hits[g_diag12_size_count] = 1;",
    "            ++g_diag12_size_count;",
    "        }",
    "    }",
    "    if (size_bytes < 128 || size_bytes > 32768) {",
    "        return;",
    "    }",
    "    const u32 head = Diag12HeadHash(data, size_bytes);",
    "    bool known = false;",
    "    for (u32 s = 0; s < g_diag12_seen_count; ++s) {",
    "        if (g_diag12_seen_key[s] == key && g_diag12_seen_size[s] == size_bytes) {",
    "            if (g_diag12_seen_head[s] == head) {",
    "                return;",
    "            }",
    "            g_diag12_seen_head[s] = head;",
    "            known = true;",
    "            break;",
    "        }",
    "    }",
    "    if (!known && g_diag12_seen_count < 512) {",
    "        g_diag12_seen_key[g_diag12_seen_count] = key;",
    "        g_diag12_seen_size[g_diag12_seen_count] = size_bytes;",
    "        g_diag12_seen_head[g_diag12_seen_count] = head;",
    "        ++g_diag12_seen_count;",
    "    }",
    "    if (g_diag12_scanned >= 20000) {",
    "        return;",
    "    }",
    "    ++g_diag12_scanned;",
    "    const u32 n = size_bytes / 4;",
    "    const u32* u = reinterpret_cast<const u32*>(data);",
    "    for (u32 i = 0; i < n; ++i) {",
    "        if (i + 1 < n) {",
    "            if ((u[i] == 1920 && u[i + 1] == 1080) || (u[i] == 1280 && u[i + 1] == 720) ||",
    "                (u[i] == 960 && u[i + 1] == 540)) {",
    "                Diag12Report(3, size_bytes, i, data, n, path);",
    "            }",
    "            const float f = data[i];",
    "            if (f >= 250.0f && f <= 100000.0f) {",
    "                const float err = Diag12Abs(data[i + 1] * f - 1.0f);",
    "                if (err <= 0.01f) {",
    "                    Diag12Report(1, size_bytes, i, data, n, path);",
    "                }",
    "            }",
    "        }",
    "        if (i % 4 == 0 && i + 16 <= n) {",
    "            const float* m = data + i;",
    "            const float l0 = m[0] * m[0] + m[1] * m[1] + m[2] * m[2];",
    "            const float l1 = m[4] * m[4] + m[5] * m[5] + m[6] * m[6];",
    "            if (Diag12Abs(l0 - 1.0f) < 0.05f && Diag12Abs(l1 - 1.0f) < 0.05f) {",
    "                const float dot = m[0] * m[4] + m[1] * m[5] + m[2] * m[6];",
    "                const float cx = m[1] * m[6] - m[2] * m[5];",
    "                const float cy = m[2] * m[4] - m[0] * m[6];",
    "                const float cz = m[0] * m[5] - m[1] * m[4];",
    "                const float cd = m[8] * cx + m[9] * cy + m[10] * cz;",
    "                if (Diag12Abs(dot) < 0.05f && Diag12Abs(cd - 1.0f) < 0.1f) {",
    "                    Diag12Report(4, size_bytes, i, data, n, path);",
    "                }",
    "            }",
    "            if (m[0] > 0.2f && m[0] < 10.0f && m[5] > 0.2f && m[5] < 10.0f &&",
    "                Diag12Abs(m[1]) < 1e-4f && Diag12Abs(m[2]) < 1e-4f &&",
    "                Diag12Abs(m[4]) < 1e-4f && Diag12Abs(m[6]) < 1e-4f && m[10] < 0.0f &&",
    "                m[10] > -3.0f &&",
    "                (Diag12Abs(m[11] - 1.0f) < 1e-3f || Diag12Abs(m[11] + 1.0f) < 1e-3f) &&",
    "                Diag12Abs(m[15]) < 1e-4f) {",
    "                Diag12Report(5, size_bytes, i, data, n, path);",
    "            }",
    "        }",
    "    }",
    "}",
    "} // namespace",
)

# --- X2: ring call site -> Diag12Scan ----------------------------------------
old_x2 = J(
    "            Diag11Scan(reinterpret_cast<const float*>(constant_ring->Data(ring->offset)),",
    "                       static_cast<u32>(ring->size), 0);",
)
new_x2 = J(
    "            Diag12Scan(reinterpret_cast<const float*>(constant_ring->Data(ring->offset)),",
    "                       static_cast<u32>(ring->size),",
    "                       ring->address != 0",
    "                           ? ring->address",
    "                           : reinterpret_cast<u64>(constant_ring->Data(ring->offset)),",
    "                       0);",
)

# --- X3: vsharp call site -> Diag12Scan --------------------------------------
old_x3 = J(
    "                Diag11Scan(reinterpret_cast<const float*>(vsharp.base_address),",
    "                           static_cast<u32>(vsharp.GetSize()), 1);",
)
new_x3 = J(
    "                Diag12Scan(reinterpret_cast<const float*>(vsharp.base_address),",
    "                           static_cast<u32>(vsharp.GetSize()), vsharp.base_address, 1);",
)

# --- X4: v12 results in the summaries -----------------------------------------
old_x4 = J(
    "            g_diag11_scanned = 0;",
    "        }",
)
new_x4 = J(
    "            g_diag11_scanned = 0;",
    '            std::printf("Rasterizer diag: v12 scan: scanned=%u hits=%u\\n",',
    "                        g_diag12_scanned, g_diag12_hit_count);",
    "            for (u32 i = 0; i < g_diag12_size_count; ++i) {",
    '                std::printf("Rasterizer diag: v12 buffer size=%u count=%u\\n",',
    "                            g_diag12_sizes[i], g_diag12_size_hits[i]);",
    "            }",
    "            g_diag12_scanned = 0;",
    "        }",
)

apply(RZ, [
    ("X1 v12 scanner", old_x1, new_x1),
    ("X2 ring call", old_x2, new_x2),
    ("X3 vsharp call", old_x3, new_x3),
    ("X4 summary v12", old_x4, new_x4),
])
print("all v12 edits applied")
