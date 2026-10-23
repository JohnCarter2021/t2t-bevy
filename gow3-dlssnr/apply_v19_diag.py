#!/usr/bin/env python3
"""v19: create the DLSS feature in its own submitted+waited buffer.

v18 forensics verdict (his last_run-13.log): the whole CPU side succeeds -
"DLSS: evaluate #1 recording: color fmt=97 1280x720 aspect=1, depth fmt=130
1280x720 aspect=2, motion fmt=83, output fmt=97 1920x1080, reset=1",
"DLSS: evaluate #1 returned 1", "Upscaler: Run completed #1" - and then the
game hangs: the GPU never finishes the first recorded DLSS frame. Every
resource is contract-correct (FP16 color via the bridge, D32 depth, RG16F
motion, FP16 output, all in eGeneral).

The one structural deviation from the NGX integration pattern: the feature
is CREATED into the frame's still-open command buffer (RecordDlss) and
EVALUATED in that same unsubmitted buffer. In the NGX sample flow (and
engine integrations) creation is recorded in a dedicated buffer that is
submitted and completed before the first evaluate.

v19 therefore creates the feature in Run(), immediately after
EnsureResources (which ends with scheduler.Finish(), so nothing is in
flight) and BEFORE Run() acquires its frame command buffer: WaitSubmitted,
ReleaseFeature, CreateFeature(scheduler.CommandBuffer(), desc),
scheduler.Finish() - submitted and waited on the spot. RecordDlss then sees
HasFeature() == true (identical desc, hdr=true as Run passes) and goes
straight to Evaluate. RecordDlss's own in-buffer create stays as the
fallback for the RunScaled path. All v18 forensics prints remain, so the
next log still brackets the evaluate.
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

old_v1 = [
    "    if (!EnsureResources(w, h, ow, oh, true)) {",
    "        failed = true;",
    "        return;",
    "    }",
]
new_v1 = [
    "    if (!EnsureResources(w, h, ow, oh, true)) {",
    "        failed = true;",
    "        return;",
    "    }",
    "    // v19: create the DLSS feature here - in its own buffer, submitted and waited",
    "    // on before this frame's recording starts. Creating it into the frame's buffer",
    "    // and evaluating in the same unsubmitted buffer hung the GPU on the first",
    "    // frame (v18 forensics: evaluate returned success and Run() completed, then",
    "    // the frame never finished executing).",
    "    if (UseDlss()) {",
    "        Dlss* dlss = Dlss::Get();",
    "        const Dlss::FeatureDesc feature_desc{w, h, ow, oh,",
    "                                             Dlss::QualityForScale(float(ow) / float(w)), true};",
    "        if (dlss && !dlss->HasFeature(feature_desc)) {",
    "            scheduler.WaitSubmitted();",
    "            dlss->ReleaseFeature();",
    "            if (dlss->CreateFeature(scheduler.CommandBuffer(), feature_desc)) {",
    "                scheduler.Finish();",
    "                reset = true;",
    '                std::printf("DLSS: feature created in its own submission (v19)\\n");',
    "            }",
    "        }",
    "    }",
]

apply(TU, [("V1 early feature creation", old_v1, new_v1)])
print("all v19 edits applied")
