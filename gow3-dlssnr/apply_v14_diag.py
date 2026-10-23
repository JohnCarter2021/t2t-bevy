#!/usr/bin/env python3
"""v14 diagnostics for the gow3_pc camera hunt (vk_rasterizer.cpp only).

v12's verdict (his session log last_run-7.log): the structure scan HIT 24
times, all kind 4 (view-like), all inside the 16,256-byte buffer at
consecutive 16-float offsets - an array of ~254 object/bone matrices (a
skinning/instancing palette: identity first, then near-identical rotations
around one translation ~(-18, 239, -298)). NOT the camera. The palette
filled every hit slot and kept consuming the scan budget as it animates, so
anything rarer never registered. One lead: a 176-byte buffer (44 floats -
room for view + projection + extras) starting with an identity 4x4.

v14 therefore:
- excludes size 16256 from Diag12Scan entirely (census still counts it);
- caps Diag12Report hits at 4 per buffer size, so one noisy size can no
  longer monopolise the hit table;
- adds Diag14Dump: for every constant buffer of 128..512 bytes on both feed
  paths, whenever the hash of its first 64 bytes changes, it prints the
  buffer's FULL float contents (size + guest key + all floats), budgeted at
  60 dumps per summary segment and 6 dumps per distinct buffer per session.
  The camera constants can then be READ directly instead of guessed.
All v9-v12 diagnostics stay in place.
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

# --- Y1: per-size hit cap inside Diag12Report --------------------------------
old_y1 = J(
    "void Diag12Report(int kind, u32 size_bytes, u32 off, const float* data, u32 n, int path) {",
    "    for (u32 h = 0; h < g_diag12_hit_count; ++h) {",
)
new_y1 = J(
    "// v14: per-size hit counters, so one noisy buffer size (the 16,256-byte matrix",
    "// palette filled all 24 hit slots in v12) cannot monopolise the hit table.",
    "u32 g_diag14_size_hits[8][2] = {};",
    "u32 g_diag14_size_hits_count = 0;",
    "",
    "void Diag12Report(int kind, u32 size_bytes, u32 off, const float* data, u32 n, int path) {",
    "    {",
    "        u32 slot = g_diag14_size_hits_count;",
    "        for (u32 s = 0; s < g_diag14_size_hits_count; ++s) {",
    "            if (g_diag14_size_hits[s][0] == size_bytes) {",
    "                slot = s;",
    "                break;",
    "            }",
    "        }",
    "        if (slot < 8) {",
    "            if (slot == g_diag14_size_hits_count) {",
    "                g_diag14_size_hits[slot][0] = size_bytes;",
    "                g_diag14_size_hits[slot][1] = 0;",
    "                ++g_diag14_size_hits_count;",
    "            }",
    "            if (g_diag14_size_hits[slot][1] >= 4) {",
    "                return;",
    "            }",
    "            ++g_diag14_size_hits[slot][1];",
    "        }",
    "    }",
    "    for (u32 h = 0; h < g_diag12_hit_count; ++h) {",
)

# --- Y2: exclude the matrix palette from the structure scan -------------------
old_y2 = J(
    "    if (size_bytes < 128 || size_bytes > 32768) {",
    "        return;",
    "    }",
    "    const u32 head = Diag12HeadHash(data, size_bytes);",
)
new_y2 = J(
    "    if (size_bytes < 128 || size_bytes > 32768 || size_bytes == 16256) {",
    "        return;",
    "    }",
    "    const u32 head = Diag12HeadHash(data, size_bytes);",
)

# --- Y3: Diag14Dump definition (end of the anonymous namespace) ---------------
old_y3 = J(
    "                Diag12Report(5, size_bytes, i, data, n, path);",
    "            }",
    "        }",
    "    }",
    "}",
    "} // namespace",
)
new_y3 = J(
    "                Diag12Report(5, size_bytes, i, data, n, path);",
    "            }",
    "        }",
    "    }",
    "}",
    "",
    "// v14: content dumps of small constant buffers. Pattern guessing has run its",
    "// course (v11 literal patterns: nothing; v12 structure: a bone palette), so",
    "// v14 prints the full float contents of every 128..512-byte constant buffer",
    "// whenever the hash of its first 64 bytes changes - the camera constants can",
    "// then be read directly from the log. Budgets: 60 dumps per summary segment,",
    "// 6 dumps per distinct buffer per session.",
    "u64 g_diag14_seen_key[256] = {};",
    "u32 g_diag14_seen_size[256] = {};",
    "u32 g_diag14_seen_head[256] = {};",
    "u32 g_diag14_seen_dumps[256] = {};",
    "u32 g_diag14_seen_count = 0;",
    "u32 g_diag14_dumps = 0;",
    "",
    "void Diag14Dump(const float* data, u32 size_bytes, u64 key) {",
    "    if (size_bytes < 128 || size_bytes > 512) {",
    "        return;",
    "    }",
    "    const u32 head = Diag12HeadHash(data, size_bytes);",
    "    u32 slot = g_diag14_seen_count;",
    "    for (u32 s = 0; s < g_diag14_seen_count; ++s) {",
    "        if (g_diag14_seen_key[s] == key && g_diag14_seen_size[s] == size_bytes) {",
    "            slot = s;",
    "            break;",
    "        }",
    "    }",
    "    if (slot < g_diag14_seen_count) {",
    "        if (g_diag14_seen_head[slot] == head || g_diag14_seen_dumps[slot] >= 6) {",
    "            return;",
    "        }",
    "        g_diag14_seen_head[slot] = head;",
    "    } else {",
    "        if (g_diag14_seen_count >= 256) {",
    "            return;",
    "        }",
    "        slot = g_diag14_seen_count++;",
    "        g_diag14_seen_key[slot] = key;",
    "        g_diag14_seen_size[slot] = size_bytes;",
    "        g_diag14_seen_head[slot] = head;",
    "        g_diag14_seen_dumps[slot] = 0;",
    "    }",
    "    if (g_diag14_dumps >= 60) {",
    "        return;",
    "    }",
    "    ++g_diag14_seen_dumps[slot];",
    "    ++g_diag14_dumps;",
    "    const u32 n = size_bytes / 4;",
    '    std::printf("Rasterizer diag: v14 dump size=%u key=%llx floats:",',
    "                size_bytes, static_cast<unsigned long long>(key));",
    "    for (u32 i = 0; i < n; ++i) {",
    '        std::printf(" %g", data[i]);',
    "    }",
    '    std::printf("\\n");',
    "}",
    "} // namespace",
)

# --- Y4: dump call at the ring site -------------------------------------------
old_y4 = J(
    "                       ring->address != 0",
    "                           ? ring->address",
    "                           : reinterpret_cast<u64>(constant_ring->Data(ring->offset)),",
    "                       0);",
)
new_y4 = J(
    "                       ring->address != 0",
    "                           ? ring->address",
    "                           : reinterpret_cast<u64>(constant_ring->Data(ring->offset)),",
    "                       0);",
    "            Diag14Dump(reinterpret_cast<const float*>(constant_ring->Data(ring->offset)),",
    "                       static_cast<u32>(ring->size),",
    "                       ring->address != 0",
    "                           ? ring->address",
    "                           : reinterpret_cast<u64>(constant_ring->Data(ring->offset)));",
)

# --- Y5: dump call at the vsharp site -----------------------------------------
old_y5 = J(
    "                Diag12Scan(reinterpret_cast<const float*>(vsharp.base_address),",
    "                           static_cast<u32>(vsharp.GetSize()), vsharp.base_address, 1);",
)
new_y5 = J(
    "                Diag12Scan(reinterpret_cast<const float*>(vsharp.base_address),",
    "                           static_cast<u32>(vsharp.GetSize()), vsharp.base_address, 1);",
    "                Diag14Dump(reinterpret_cast<const float*>(vsharp.base_address),",
    "                           static_cast<u32>(vsharp.GetSize()), vsharp.base_address);",
)

# --- Y6: dump count in the summaries ------------------------------------------
old_y6 = J(
    "            g_diag12_scanned = 0;",
    "        }",
)
new_y6 = J(
    "            g_diag12_scanned = 0;",
    '            std::printf("Rasterizer diag: v14 dumps this segment=%u\\n", g_diag14_dumps);',
    "            g_diag14_dumps = 0;",
    "        }",
)

apply(RZ, [
    ("Y1 per-size hit cap", old_y1, new_y1),
    ("Y2 palette excluded", old_y2, new_y2),
    ("Y3 dump function", old_y3, new_y3),
    ("Y4 ring dump call", old_y4, new_y4),
    ("Y5 vsharp dump call", old_y5, new_y5),
    ("Y6 summary dumps", old_y6, new_y6),
])
print("all v14 edits applied")
