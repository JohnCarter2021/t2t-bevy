#!/usr/bin/env python3
"""v18: evaluate forensics (no behavior change).

v17's log: the bridge engaged on the reduced path ("v16 scene bridge: 1280x720
format 43 -> RGBA16F"), the DLSS feature was created, and the game STILL froze
at the first evaluate - with a verified FP16 color input. The sRGB-format
hypothesis is therefore falsified as the (sole) cause. What the logs cannot
yet distinguish:
  (a) the NGX evaluate call never returns on the CPU (hang inside the call -
      e.g. it waits for the feature-creation command buffer to complete, but
      creation was recorded into the same still-open buffer), or
  (b) evaluate returns and Run() completes, but the GPU hangs executing the
      recorded frame (the next present/WaitSubmitted then blocks forever).
v18 brackets the call chain with prints (first 3 frames):
  - vk_dlss.cpp Dlss::Evaluate: entry print with all four resources'
    format/size/aspect, exit print with the NGX result;
  - RecordDlss: prints around scheduler.WaitSubmitted() in the create path;
  - Run(): a completion print after the merge/copy-back, inside Run().
Whichever print is LAST in the next log names the hanging stage.
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

DL = "gpu/shadps4/video_core/renderer_vulkan/vk_dlss.cpp"
TU = "gpu/shadps4/video_core/renderer_vulkan/vk_temporal_upscaler.cpp"

old_d1 = [
    "    const Gow3DlssEvaluate parameters{image(frame.color),  image(frame.depth), image(frame.motion),",
    "                                    image(frame.output), frame.jitter_x,     frame.jitter_y,",
    "                                    frame.reset ? 1 : 0, frame.frame_ms,     frame.sharpness};",
    "    return impl->api->Evaluate(command, &parameters) != 0;",
]
new_d1 = [
    "    const Gow3DlssEvaluate parameters{image(frame.color),  image(frame.depth), image(frame.motion),",
    "                                    image(frame.output), frame.jitter_x,     frame.jitter_y,",
    "                                    frame.reset ? 1 : 0, frame.frame_ms,     frame.sharpness};",
    "    // v18 forensics: bracket the NGX evaluate call (first 3) - if the entry print",
    "    // appears without the exit print, the call itself never returns.",
    "    static u32 eval_count = 0;",
    "    const u32 eval_no = ++eval_count;",
    "    if (eval_no <= 3) {",
    '        std::printf("DLSS: evaluate #%u recording: color fmt=%d %ux%u aspect=%u, depth fmt=%d '
                '%ux%u aspect=%u, motion fmt=%d %ux%u, output fmt=%d %ux%u, reset=%d\\n",',
    "                    eval_no, static_cast<int>(frame.color.format), frame.color.width,",
    "                    frame.color.height, static_cast<u32>(frame.color.aspect),",
    "                    static_cast<int>(frame.depth.format), frame.depth.width, frame.depth.height,",
    "                    static_cast<u32>(frame.depth.aspect), static_cast<int>(frame.motion.format),",
    "                    frame.motion.width, frame.motion.height, static_cast<int>(frame.output.format),",
    "                    frame.output.width, frame.output.height, frame.reset ? 1 : 0);",
    "    }",
    "    const bool eval_ok = impl->api->Evaluate(command, &parameters) != 0;",
    "    if (eval_no <= 3) {",
    '        std::printf("DLSS: evaluate #%u returned %d\\n", eval_no, eval_ok ? 1 : 0);',
    "    }",
    "    return eval_ok;",
]

old_d2 = [
    "    if (!dlss->HasFeature(desc)) {",
    "        // The previous feature may still be in use by submitted work; `cmdbuf` stays open.",
    "        scheduler.WaitSubmitted();",
    "        dlss->ReleaseFeature();",
    "        ok = dlss->CreateFeature(cmdbuf, desc);",
    "        reset = true;",
    "    }",
]
new_d2 = [
    "    if (!dlss->HasFeature(desc)) {",
    "        // The previous feature may still be in use by submitted work; `cmdbuf` stays open.",
    '        std::printf("DLSS: feature create: waiting for submitted work\\n");',
    "        scheduler.WaitSubmitted();",
    '        std::printf("DLSS: feature create: wait done, recording creation\\n");',
    "        dlss->ReleaseFeature();",
    "        ok = dlss->CreateFeature(cmdbuf, desc);",
    "        reset = true;",
    "    }",
]

old_d3 = [
    "            cmdbuf.pipelineBarrier2({.imageMemoryBarrierCount = 1, .pImageMemoryBarriers = &done});",
    "        }",
    "    }",
    "}",
]
new_d3 = [
    "            cmdbuf.pipelineBarrier2({.imageMemoryBarrierCount = 1, .pImageMemoryBarriers = &done});",
    "        }",
    "    }",
    "    // v18 forensics: Run() got all the way through recording this frame.",
    "    if (dispatched) {",
    "        static u32 completed = 0;",
    "        if (++completed <= 3) {",
    '            std::printf("Upscaler: Run completed #%u (frame recorded with DLSS)\\n", completed);',
    "        }",
    "    }",
    "}",
]

apply(DL, [("D1 evaluate brackets", old_d1, new_d1)])
apply(TU, [
    ("D2 create wait brackets", old_d2, new_d2),
    ("D3 Run completion print", old_d3, new_d3),
])
print("all v18 edits applied")
