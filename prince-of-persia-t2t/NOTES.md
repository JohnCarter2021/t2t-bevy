# Prince of Persia: The Two Thrones — RTX Remix, Sands of Time template check
Date: 2026-10-22

## Source checked
- Sands of Time ModDB RTX page: Status Playable, CI Build runtime, conf updated ~123 days ago.
- GitHub: kaminoer/pop-sot-rtx (Simulatte / kaminoer) — "fixed-function pipeline to enable compatibility with RTX Remix"
  - Repo root contains README.md + rtx.conf (checked in full, 84 lines)
  - Releases: v0.2 (latest, fixed hair alpha, Home-key settings GUI), v0.1a (menu clipping fix)
- Two Thrones ModDB RTX page: Status Partially Playable, CI Build, NO conf file uploaded yet.
- Warrior Within ModDB RTX page: same status / same situation as Two Thrones.

## The key finding — rtx.conf is NOT the main work
SoT does NOT work with vanilla Remix. The mod "converts SoT's shader based rendering
pipeline into fixed-function rendering, which enables compatibility with RTX Remix."
Install order (SoT):
1. Game (Ubisoft Connect, English only — other versions not supported)
2. Sands of Time Fix Compilation
3. RTX Remix runtime unpacked into game folder, Remix d3d9.dll renamed to d3d9_remix.dll
4. Mod's own d3d9.dll placed in game folder (this is the pipeline-conversion wrapper)
5. Optional: author's rtx.conf next to d3d9.dll — "initial tagged light, UI, sky textures, NOT complete"

For Two Thrones AND Warrior Within, ModDB states verbatim that compatibility
"is being developed by Simulatte (kaminoer) using the same fixed-function pipeline
conversion approach as the Sands of Time mod, a public release is coming soon"
— tested 14/04/2026, WIP, "A release is not yet available."

=> There is nothing to port from SoT's d3d9.dll by config alone. T2T needs its own
conversion wrapper (different exe / shader set, even on the same Jade engine family),
and that wrapper does not exist publicly yet.

## What in SoT's rtx.conf transfers to T2T — and what does not

### Transfers (kept in the starter rtx.conf next to this file)
Generic runtime settings only, ~26 lines:
- rtx.useVertexCapture = False, rtx.useVertexCapturedNormals = False,
  rtx.useWorldMatricesForShaders = False
- rtx.fallbackLightMode = 0, rtx.graphicsPreset = 0
- Path tracing: pathMinBounces 1, pathMaxBounces 4, skyBrightness 1,
  enableAlphaBlend True, denoiseDirectAndIndirectLightingSeparately True,
  replaceDirectSpecularHitTWithIndirectSpecularHitT True,
  enableTransmissionApproximationInIndirectRays False,
  enableUnorderedEmissiveParticlesInIndirectRays True
- Neural Radiance Cache: qualityPreset 2, trainingMaxPathBounces 9,
  terminationHeuristicThreshold 0.1, smallestResolvableFeatureSizeMeters 0.01,
  targetNumTrainingIterations 4
- Effect lights: radius 0.01, intensity 0.00015, color 1 / 0.705882 / 0.264706
- volumetrics.froxelGridResolutionScale = 4, rtx.dlfg.enable = True,
  camera.enableFreeCamera = False, gui.showLegacyTextureGui = False

### Does NOT transfer (dropped — SoT-texture-specific, will never match in T2T)
All of these are lists of texture hashes computed from SoT's own textures.
In T2T they are silent no-ops at best:
- rtx.lightConverter (18 hashes)
- rtx.ignoreTextures (8)
- rtx.skyBoxTextures (1)
- rtx.decalTextures (19)
- rtx.uiTextures (~37)
- rtx.animatedWaterTextures (3)
- rtx.particleTextures (8) + rtx.beamTextures (2)
- rtx.ignoreAlphaOnTextures (1)
Also dropped: rtx.camera.freeCameraPosition / Yaw / Pitch — coordinates of one SoT scene.

T2T's equivalents can only be built by capturing T2T in the Remix Toolkit and
tagging its own textures (lights / UI / sky / decals / particles) — that tagging
is the actual modding work, per game.

## Known issues inherited from SoT (expect the same class of problems in T2T)
- Water = solid blue placeholder
- Time-powers screen overlay effects greatly simplified
- Fog disabled by default
- Multiplicative-blend particles skipped; complex vertex-shader particle motion
  (ripples, sparkles) reduced to flat textured quads
- Some effects wrong colour / no animation / distorted / missing
- Captured joints have no hierarchy (spiky skeletons in Toolkit)
- Indexed blending for skinned meshes unsupported with RT off — character geometry
  distorts if RT is disabled; F10 toggles CPU skinning (skeletons not captured then)
- SoT release crashes capturing zones with >2 skeletons (ring buffer limit)

## Bottom line for T2T
- Starter rtx.conf in this folder = the transferable generic half. Safe to drop in
  next to a Remix runtime for a first hook test.
- It will NOT by itself make T2T Remix-playable, because the fixed-function
  conversion d3d9.dll for T2T has not been released.
- Options: (a) wait / follow Simulatte's T2T release (Discord thread linked on ModDB),
  (b) run the starter conf + vanilla CI runtime anyway and log what hooks/breaks —
  useful data, not a playable result, or (c) look at adapting SoT's conversion
  source — a real reverse-engineering project, not a config job.


---

# Addendum 2026-10-22 17:21 — user-supplied repos: "Use all these"

User supplied (duplicates removed):
- https://github.com/dekamaru (profile)
- https://github.com/kaminoer/PoP-Mod-Manager
- https://github.com/kaminoer/Jade-Toolkit
- https://github.com/dekamaru/openjade
- https://github.com/kaminoer/popaudiostudio
- https://github.com/POPTutorials/poptutorialsmods (listed twice)

Cloning all of them (plus kaminoer/pop-sot-rtx and dekamaru/openjade-mods, his other
PoP repo) into ~/workspace/pop-two-thrones-rtx/tools/ — in progress 2026-10-22.

## Role map (from each repo's README, checked 2026-10-22)

1. **kaminoer/Jade-Toolkit** — the authoring tool. Modding toolkit for the whole
   Sands trilogy. ARCHIVED Sep 2026 ("New tools coming..."), author warns
   "IS NOT FULLY WORKING" — texture and mesh swap mostly work, level editor and
   other tabs not fully ported. C++ rewrite of his Python scripts. Output format:
   .jtmod mods. Use for: extracting / swapping T2T textures and meshes — the asset
   side that a Remix remaster also needs (PBR replacements start as texture swaps).

2. **kaminoer/PoP-Mod-Manager** (73 stars, 16 releases) — the install layer.
   "Prince of Persia Sands trilogy mod manager for mods created with Jade Toolkit."
   Ships PMMMod manifest generator. Use for: installing/applying .jtmod mods to the
   T2T install cleanly (and backing out), instead of hand-editing game .bf files.

3. **POPTutorials/poptutorialsmods** (created 2026-10-17) — example content for
   the Mod Manager: 8 .jtmod files + pmm_manifest.json — Devil May Cry sword swaps
   onto T2T secondary swords (Lion / Water / Spider / Snake / Scorpion), an
   "Incredible HUD" mod, a Prince mod, Dahaka mod. Use for: proof the .jtmod +
   manifest pipeline works end-to-end, and as format examples if we author our own.

4. **dekamaru/openjade** (68 stars, 17 releases) — the alternative engine path.
   Independent Jade engine implementation supporting SoT + WW + T2T explicitly.
   Native x64, frame-rate-independent logic (60+ FPS), **Vulkan rendering**,
   OpenAL 3D audio, widescreen/ultrawide, full controller support, faster loading,
   replace .BIK videos with .mp4 and artwork with .png/.jpg. Install: drop
   openjade.zip into game dir, run openjade.exe.
   IMPORTANT: OpenJade does NOT combine with RTX Remix — Remix hooks d3d9, and
   OpenJade replaces rendering with Vulkan. It is a competing route:
   (a) Remix route = original exe + fixed-function conversion (Simulatte, T2T not
   released) + path tracing; (b) OpenJade route = playable modern T2T today, no
   path tracing — RT/DLSS there would be native Vulkan work like the gow3 project.

5. **dekamaru/openjade-mods** — collection of trilogy mods for OpenJade. Pairs
   with (4) if the OpenJade route is chosen.

6. **kaminoer/popaudiostudio** (beta, 4 releases) — audio modding for the trilogy.
   Use for: soundtrack / voice / SFX extraction and replacement. Peripheral to
   Remix graphics, relevant to a full remaster package.

7. **dekamaru (profile)** — beyond openjade / openjade-mods his other repos
   (space_invaders, tagger-bot, voiceapp, iced-layout) are unrelated to PoP.

## How they fit together for T2T
Asset pipeline: Jade-Toolkit (author/extract) -> .jtmod -> PoP-Mod-Manager (install)
               + PopAudioStudio (audio side)
Engine choice: original exe (Remix route, waiting on Simulatte's T2T conversion)
               OR OpenJade exe (modern Vulkan route, available now)
Reference:     kaminoer/pop-sot-rtx (the SoT Remix conversion pattern)


---

# Addendum 2026-10-22 17:35 — the user's Drive zip, checked

File: Prince.of.Persia.The.Two.Thrones.zip (Drive, uploaded 2026-10-22 13:42 IST)
- Size on Drive and on disk: 1,153,728,917 bytes — match.
- SHA-256 (zip): 44136710784340919031e0125cb67e162d0dffb39a654bbb69a439eb1b16bfaf
- Local copy: ~/workspace/pop-two-thrones-rtx/incoming/

## Contents (7 entries only — it is an installer package, not a game folder)
- Prince.of.Persia.The.Two.Thrones/setup_pop_the_two_thrones_2.0.0.5.exe (1,142,699,296 bytes, 2015-11-07)
  - SHA-256: 0cd3c602b162cb67bc3a9318f2979b0c11f8c1eb11ebe27b768f76735328ffc1
  - PE32 GUI; strings confirm: Inno Setup 5.5.0, "Prince of Persia The Two Thrones Setup",
    "(c) 2012 GOG.com LTD" — i.e. the GOG offline installer, GOG build 2.0.0.5.
- HOW TO RUN GAME!!.txt — "run setup as administrator; if missing dll errors,
  install the 3 programs in _Redist"
- _Redist/dxwebsetup.exe, vcredist_x86.exe, vcredist_x64.exe
- The 2020-05-11 folder dates show this is a repack of the GOG installer, not a
  fresh GOG download — hashes above are the record of exactly what was supplied.

## What this version means
- GOG = DRM-free: no StarForce. That matters — the retail disc pop3.exe is
  ~6.05 MB (StarForce-wrapped, with RegistrationReminder/amir files), while the
  GOG install lays down pop3.exe ~3.29 MB + PrinceOfPersia.exe launcher (172 KB)
  + unins000.exe, game version 1.1, uninstall ID 1207659091_is1 (GOG ID 1207659091).
  Cross-checked against public GOG-install records.
- So this is the good base we hoped for: same class as the DRM-free copy the
  Remix drop-in test wanted. Jade-Toolkit / Mod Manager / OpenJade all work
  against an installed folder like this.

## Limits of this check (honest scope)
- The Inno installer's inner file list was NOT extracted on this Linux VM —
  no Inno extractor was available here, so pop3.exe itself was not inspected
  byte-level. Installer identity/integrity are verified; inner-game verification
  happens on the installed folder on Windows (list in the install step below).

## Install + first-check step (Windows, user's PC)
1. Run setup_pop_the_two_thrones_2.0.0.5.exe as admin, install to a short path
   e.g. C:\Games\Prince of Persia T2T (avoid Program Files UAC issues the GOG
   version is known for).
2. Confirm installed files: POP3.exe (~3.29 MB), PrinceOfPersia.exe, the .bf
   data files — send a folder screenshot / listing back for verification.
3. Then stage, in order: starter rtx.conf (this folder) for the Remix hook test,
   OpenJade release for the Vulkan route, PoP-Mod-Manager for .jtmod mods.


---

# Addendum 2026-10-22 18:44 — extracted game zip, VERIFIED

File: "Prince of Persia The Two Thrones.zip" (Drive, uploaded 2026-10-22 17:43 IST)
- Size: 1,311,862,809 bytes (Drive = disk, match)
- SHA-256 (zip): 76198b16f1292c487ed5cb077145e9306f4e8c211a6f4f7fbc85ab450b716fa7
- Extracted to: incoming/extracted/Prince of Persia The Two Thrones/ (167 files, ~1.5 GB)

## Identity — this IS the GOG build, confirmed from the files themselves
- pop3.exe: 3,453,440 bytes, dated 2012-06-11 (GOG rebuild), FileVersion resource
  1.0.0.188, UPX-compressed PE32. SHA-256 5f48434cc412aa776cf995ed6cc215c258c5d4704c44748e356408aa2ad3a4aa
  => NOT the ~6.05 MB StarForce retail exe. No StarForce anywhere in the folder.
- PrinceOfPersia.exe launcher: 176,128 bytes (172 KB) — exact GOG/retail launcher size.
  SHA-256 547825eb9863441bebde28edbcb3a35b4d23ad1fa6d6a411d2ec252d507a7e32
- GOG markers in folder: goggame.dll, gog.ico, GameuxInstallHelper.dll, unins000.*
  (unins000.exe dated 2026-10-22 17:40 = the user's own install that day).
- Data: pop3.bf 497,424,384 bytes, magic "BIG\0" (Jade BIG-file format) —
  SHA-256 00134ea93ccf27f29ede40d3521439ca8f8c0d38bc02fdfd0d18042e7929b600
  POPData.bf 22,437 bytes; POP3.dat 12,758,256 (SHA-256 0ab98384…6b410)
  Video/ 609 MB, Sound/ 330 MB, MENU/ENGLISH, POP3Profiles (Default+Safe),
  POP3Config.ini present. Hash list: game_hashes.txt in project root.

## Rendering path — Remix-relevant, from pop3.exe's own import table
- Imports d3d9.dll -> Direct3DCreate9. A d3d9.dll placed next to pop3.exe is the
  one that loads — the standard Remix hook point works on this copy.
- No d3d9.dll / dxgi.dll / wrapper present in the folder: CLEAN, nothing to conflict.
- Imports amir.dll (SF_Init), binkw32.dll, dinput8.dll, dsound.dll, eax.dll,
  MFC71/msvcp71/msvcr71 (VS2003 runtime — matches the _Redist in the installer zip).
- Strings confirm runtime shader assembly: D3DXAssemble, D3DXPSGP,
  texShaderConst — i.e. the shader-based Jade pipeline, exactly as expected;
  this is why Remix needs Simulatte's fixed-function conversion, not just a conf.
- UPX packing on pop3.exe is harmless for Remix (hook is at d3d9 load, in-memory
  after unpack) and for Jade-Toolkit (it works on the .bf files, not the exe).

## Verdict
Extracted copy is verified-good and is the right base for all three routes:
Remix hook test (starter rtx.conf ready), OpenJade (drop into this folder),
Jade-Toolkit / Mod Manager (.bf format confirmed BIG).
Next staging step is user's choice — see STATUS.md.


---

# Addendum 2026-10-22 18:50 — NEW DIRECTION: full reimplementation in Rust + Bevy
User's instruction: "Create a 1:1 copy of this game in Rust with Bevy you can
use everything like REA and all the tools." Model = OpenJade-style engine
reimplementation reading the ORIGINAL data files from his own copy (no asset
redistribution). Project: t2t-bevy/ in this folder (README + docs/ROADMAP.md
M1–M7 there).

M1 DONE same day:
- Rust 1.99.1 installed (rustup, ~/.cargo).
- src/bigfile.rs: complete Jade BIG reader ported from the format spec in
  Jade-Toolkit's BigFile.cpp, cross-validated on the real pop3.bf.
  Format facts: header 44 B (v38, size_of_fat 231061 slots, num_fat 1,
  universe_key 0xcc00008e), FATdes at 44 (pos_fat 0x44), FileTable (pos,key)
  8 B, FileTableExt 84 B (OLD size auto-detected), DirTable 84 B.
  Result: 6,648 active files, 5 dirs (ROOT, Bin, EngineDatas,
  04 Technical Bank, LoadScreen).
- bf_ls CLI lists/extracts entries; integration test on the real pop3.bf
  PASSES (cargo test green, no warnings).
- REA: opened pop3.exe — PE x86, SHA-256 matches ours exactly (5f48434c…4aa).
  REA sessions don't persist across CLI calls and no native decompile
  provider (Ghidra) is installed on this VM — deeper exe RE is a later
  setup step, formats come from Jade-Toolkit meanwhile.
- t2t_game: Bevy 0.16 app (feature bevy-app) boots + indexes archive at
  Startup. Build on this VM: see t2t-bevy build log (X11/ALSA/udev dev
  libs are the usual Linux blocker; Windows build is the target anyway).
Next: M2 — LZO stream decompression + size.grs, classify all 6,648 keys.


## M2 DONE — 2026-10-22 19:15-19:35 (user: "go ahead")
- src/lzo.rs: LZO1X decoder (state-machine port of lzo1x_decompress_safe);
  src/jade_stream.rs: Jade block-stream walk/decompress + size.grs parser.
- Stream format: u32 bin_file_size (= entry length), blocks of
  (dec_size, comp_size, data), comp>=dec means stored raw, dec==0
  terminator; block cap 128 KiB. size.grs = raw (key, terminator_offset)
  table — 6,655 rows / 6,642 unique keys; NOT decompressed length.
- VERIFIED on real pop3.bf: 6,639 LZO entries, 0 decode failures,
  size.grs agreement 6,639/6,639; 435.6 MB stored -> 758.2 MB out.
  579 wow worlds = 729.9 MB (96%); biggest = 740_Well_TopWell 9.17 MB,
  100_Quai, Menu3D, 800_ClimbUp_P01, 730_Garden.
- Only 9 raw entries (2 PS2 icons, versiontag, TimeLine, 2 PS2 loading
  screens, one GFX_wow outlier idx 6235, WOLInfo, size.grs).
- Tests: tests/lzo_real.rs green; full suite green (release).
- Next: M3 — wow record-stream parsing -> textures + static geometry ->
  first rendered area in Bevy. Spec in t2t-bevy/docs/FORMATS.md.


## M3 DONE — 2026-10-22 19:36-20:12 (user: "go ahead")
- New modules: wow.rs (record walker, port of WowStream/SubEntry),
  texture.rs (44B header, magics FFFFFFFF/CAD01234/C0DEC0DE, formats
  0/1/5/6/7/11 with toolkit-parity DXT decode), geometry.rs (GEO v7/v8
  PC; MRM marker 0xC0DE2002 detected-not-parsed), png.rs (stored-zlib
  PNG writer), wow_ls bin (survey + --save-tex/--save-geo/--wireframe).
- Menu3D wow: 775 records, 128 textures, 76 geometries, 0 MRM.
  Record 772 DXT5 128x128 flame atlas -> PNG, VISUALLY CONFIRMED.
  Record 398 GEO (4,114 pts, 2,835 tris, 24 elems) -> wireframe PNG =
  coherent architecture; OBJ exported (out/ folder).
- Texture stubs: many records are 56-60B header-only; full pixel data
  sits in sibling records (771-773 in Menu3D) or father BF entries.
  Binding stubs to pixels + GAO transforms = M4 first tasks.
- t2t_game: loads wow, builds de-indexed Bevy mesh from largest GEO
  (normals/UVs, AABB-centered, 20-unit scale), camera + directional
  light. Release build CLEAN after one import fix (bevy::render::mesh).
  Runtime here still stops at winit (headless VM) — mesh path executes
  before that. All 3 real-data test suites pass (bigfile/lzo/wow).


## M4 DONE — 2026-10-22 20:15-20:35 (user: "go ahead")
- Modules: gao.rs (GAO: 16B header + name@16, 68B column-major global
  matrix @ 16+name_size+10, BV 24/48B, visual gro/grm keys, hierarchy),
  material.rs (gro 3 single tex@+24, 4 multi positional sub-keys,
  5 multitex + LEAF reader: kind word 4-9 -> tex key offset
  40/40/40/42/50/63 — the toolkit exporters' resolve_texture_key;
  the plain multitex parse resolves NOTHING in T2T, the leaf reader
  resolves everything), assemble.rs (record index, geo-group member
  scan, world transform, per-element texture resolution),
  wow_render.rs (software rasterizer: Z-up perspective, persp-correct
  UV, z-buffer, lambert).
- Menu3D ASSEMBLED: 353 GAOs (290 visual) -> 102 geos, 84,887 tris,
  798 elements, 798/798 texture keys resolved to pixel data in-wow,
  0 stub-only, 0 missing. Renders VISUALLY CONFIRMED: panoramic
  skyline/cloud skybox ring + Tower of Babylon island city = the
  game's main-menu scene (out/menu3d_render.png + _close.png).
- t2t_game: merged assembled world mesh, vertex colours = per-element
  texture averages, Jade Z-up -> Bevy Y-up swap, camera widened.
- Roadmap rescoped: M4 = assembly (done); character/animation = M5;
  gameplay M6, audio/menus M7, campaign M8.


## M5 DONE (skin+skeleton) — 2026-10-22 20:58-21:20 (user: "go ahead")
- geometry.rs: MRM skin block ported (offset 36 = flags u16 + list
  count u16; per list bone_idx/num_verts/bind matrix 16f/type u32/
  (vert,weight) u16 pairs; post-block u32 gates normals). GeoError::
  MrmSkin retired. Weights = u16 fractions of 0xFFFF (verified:
  Kaileena all 1,443 verts covered, sums <= 0x10000).
- 100_Quai characters: M_Human_Guard (1,017 pts/42 bones),
  m_Kaileena (1,443 pts, 2,310 tris, 49 bones), m_Babylonian_Soldier
  (837 pts/33 bones). Character units are METRES (Kaileena 1.84 tall)
  vs level ~100x units; GAO matrices in-wow are NOT char transforms
  (runtime skeleton root places them) — render chars with --local.
- Skeleton = GAO hierarchy: 50 "B_Kaileena ..." GAOs (49 skinned +
  1 pure node), father keys + local matrices; full finger chains,
  hair/cloth MOD bones. "100_Quai_FAKE_Kaileena" = static stand-in.
- Kaileena bind-pose render VISUALLY CONFIRMED (out/kaileena.png):
  headpiece, hair, choker, gown+sash+belt, cape, arm tattoos, feet.
  Note: her textures render desaturated in wow_render (likely a
  luminance/lightmap channel choice in materials) — polish later.
- wow_render: --only-gao (exact when name ends .gao), --local,
  --find-geo KEY debug. tests/wow_quai.rs asserts the census;
  all 4 real-data suites pass.
- OPEN (deep M5 remainder): animation clip banks — Jade-Toolkit has
  NO animation parser to port; needs format RE / REA on pop3.exe.
  The Prince's own mesh: survey a Prince-bearing sector next round
  (he loads per-level like all characters; same parse path).


## ANIMATION FORMAT HUNT — 2026-10-22 22:34-23:10 (user: "go for the animation format")
- Located: toolkit classify rule gro>>16==0x0002 => ANIMATION; TRL =
  the Python predecessor's parse_trl (track count only; source not
  public; C++ toolkit excludes animations everywhere by design).
- Quai census: Kaileena family clips gro 0x2002b x3 (34-56KB) +
  0x20006 smalls; other families similar. Samples saved:
  t2t-bevy/out/anim/rec{883,872,941,874,498}.bin (wow_ls --save-record).
- pop3.exe UPX-unpacked (apt upx-ucl) for strings: only game-level
  messages, no format internals. Leaked Rabbids Jade source exists
  publicly per raydev.wiki but NOT used (leaked proprietary source;
  project stays clean-room from data files).
- FINDINGS: bodies bit-packed; 28B block headers
  (03 ?? ?? ?? (10|20) 00 00 00 02 00 00 00 ...) tile record 883
  exactly (6 blocks, 28/44B alternating) and recur 70x in rec872;
  payload keys = 12B f32 triples quantized to reduced mantissa
  (0.04443359375 exact), bit-offset within payloads; repeated
  identical triples = static tracks. gro 0x20003 = different
  sub-format (0 hits for the block pattern).
- NOT DONE: header bit grammar, quantization widths, timing, track
  -> bone mapping, hence no decoder yet and no animated pose. Next
  session entry point: differential probing of same-rig clips +
  REA/disassembly of the ANI loader in the unpacked exe.


## ANIMATION HUNT 2 — FIRST FAMILY DECODED — 2026-10-22 22:54
- The gro 0x20006/0x20036 small records are arrays of 72B units
  (28B A-block + 44B B-block), one per bone: [0]=03, [1]=FA (FE on
  unit0), idx u16 @2 (FFFF on unit0, then 0..), size words 16/32 @4,
  channel 2 @8. A: compressed rotation 4B @19 duplicated @24 (zero
  sep @23). B: translation 3x f32 @47 duplicated @60 (zero sep @59).
- Verified on Kaileena rec key 0x4200359f: 35 units = 2,520B exact,
  indices 0..33, translations humanoid-scale with mirrored L/R
  pairs (bones 2/6: (-0.088,0.042,+/-0.133)); rec 0x420035a5 = 3
  units. src/anim.rs + tests/anim_real.rs GREEN; suite total 5/5.
- A-rotation packing: smallest-three quat (2+10+10+10) and i8
  fixed-point euler both FAILED norm checks — still open, do not
  assume. Track->bone mapping open (35 tracks vs 49 bones).
  0x2002b long clips: different grammar after block 0 — still open.
- Comparative check done: B triples are NOT the bone GAOs' local
  translations (those are chain-cumulative x-dominant values) —
  they are genuine animation pose values.


## ANIMATION HUNT 3 — ROTATION PACKING CRACKED — 2026-10-22 23:10
- A-block = smallest-three quaternion: idx = top 2 bits, 3x10-bit
  fields at shifts 20/10/0, range [-1/sqrt2, +1/sqrt2], largest =
  sqrt(1 - sumsq). My earlier rejection used wrong shifts (22/12/2)
  — own arithmetic slip, now corrected in the record.
- Validation: harvested 179 units across wows (wow_ls --dump-pose,
  out/anim/pose_harvest.txt): 179/179 sumsq<=1, 0 largest-invariant
  violations, w-largest 122/179, root raw 0xe0080200 = identity,
  several units match bind bone rotations at d<0.002.
- anim::decode_rotation implemented; PoseUnit.rotation added;
  tests green (suite anim_real incl. norm + invariant asserts).
- Bind-skeleton composition VERIFIED in Python: GAO locals compose
  to her exact T-pose (body axis +X, arms +/-Z, legs -X, 1.84m).
- NOT solved: track->bone mapping (record order composition =
  nonsense; candidates: skin-block order, core-bone order),
  B-block frame semantics (|z|=3.59 outliers), replace-vs-delta
  convention. Next: LBS skinning sweep in wow_render.


## ANIMATION HUNT 4 — SKINNING PIPELINE VERIFIED — 2026-10-22 23:28
- pose_test.rs built: bind check shows geo_global = pelvis GAO
  global (axis swap (z,-y,x) + pelvis t) x composed GAO locals;
  skin matrix = geo_global x GEO bind (true inverse-binds);
  identity to <1e-4 for 45/49 bones. Exceptions bone_idx 45-48
  (BackCloth R04 + loincloth): binds match NEXT bone's global —
  cloth quirk, documented.
- MRM weights sum to 0.25/0.5/0.75 per vertex — NORMALIZE. With
  that, skinned bind pose AABB == raw AABB exactly; render
  out/kaileena_skinned_bind.png = perfect standing Kaileena.
  tests/skin_real.rs locks the pipeline in.
- Mapping sweeps (record/core/skin order x replace/rot-only/delta)
  ALL fail for record 0x4200359f on Kaileena. m3c2's "sitting
  figure" was a false positive (bind torso + bent legs + exploded
  cloth). Harvest shows unit counts 3/4/7/20/35/36/39 across
  records => generic track lists; track->object binding lives in
  a referencing record (TRL graph), yet to find. 0x2002b clips
  remain the per-character target.


## ANIMATION HUNT 5 — BINDING RECORDS FOUND — 2026-10-23 00:00
- BF-wide key scan (bf_ls --find-key, 4s for the whole archive):
  animation keys are referenced ONLY inside their own wow, in a
  cluster of 24 descriptor records (Quai records 744-769, 64B
  each): payload [01 ff][anim data key][zeros][f32 15.0][tail =
  header-record key or 0xffffffff].
- Header records (142B, keys like 0x42000313/0x42000419) sit
  adjacent to clips in record order (873 after clip 871 + list
  872) and hold a 42-entry [i][v][fd] table, i=1..42, v in 1..50
  or 0xff — the track->bone remap table (1-based bones).
- Pairs so far: desc 745/747 -> (tracklist 0x4200359f, hdr
  0x42000313); desc 746 -> (clip 0x420035a0, hdr 0x42000419);
  desc 744 -> (clip 0x420035a7, no header / flags tail).
- Tools added: wow_ls --xref/--list-spans/--save-raw, bf_ls
  --find-key. Samples in t2t-bevy/out/anim/.
- NEXT: parse all descriptors+headers; verify remap vs bone
  counts; then 0x2002b clip key-run grammar under a known header.


## ANIMATION HUNT 6 — DESCRIPTORS/HEADERS PARSED — 2026-10-23 00:05
- Descriptor type word = 0x????FF01 (???? = data key low16);
  payload: f32 @+34 (15.0 mostly, 13.0 for 3 of char-2's), header
  key @+42 (or 0xffffffff = none). Full Quai table in FORMATS.md.
- Headers 873/875: 42 entries, 14 mapped, v in {1,3,4,5,7,8,9,
  10,17,18,19,20,27,50} (+28 in 875) — track->bone (1-based)
  reading survives; 42 entries vs 35/36 units unexplained.
  Headers 879/881: 49 entries all-unmapped placeholders.
  Headers 884/886: single entry.
- Clip rec874 probe: A-style header at 0, second header shape at
  +32 (size 0xf0, count 15 == descriptor f!), zero runs, packed
  key body; only one unit-style header in 34KB.
- NEXT: segment clip body (track sections?), test remap by
  rendering tracklist 9f under header 313's map via pose_test.


## ANIMATION HUNT 7 — CLIP TWO-SECTION STRUCTURE — 2026-10-23 00:25
- Five clips verified: two opening sections; section 2's size
  word = track_count x 16 in ALL (a0 15 tracks, a2 21, a7 37,
  6d46 22, 6d4b 2). Section 1: size = (count/2)x16, count 2/4.
- Section 2's 16B/track rows: 0x008000xx word (xx plausibly the
  per-track key count) + metre-range f32s (first-key data?).
- rec877 payload has a leading variant byte (grammar shifted).
- NEXT: pin the 16B row layout; use key counts as checksum to
  walk the packed stream; then decode keys -> remap -> render.


## ANIMATION HUNT 8 — FOUR SECTIONS + KEY MOTIF — 2026-10-23 00:30
- rec874 lead scan: sections @0 (16,2), @32 (240,15 tracks),
  @288 (2032,254), @1830 (3216,201) — two SEC1+SEC2 pairs
  (rotation side + translation side hypothesis); 254/201 may
  be total key counts per side.
- Rows align at section2+32: 14 x [count 0x008000xx][vec3];
  15th track = root, likely section 1's subject (as in the
  unit records' root unit).
- Packed stream shows a 6B motif [u16 delta][u32 value]
  repeating — (frame-delta, quat) keys; static channels
  repeat identical values.
- NEXT: Rust walker in anim.rs; consume stream per track
  counts; record end = checksum. Then decode + remap + render.


## ANIMATION HUNT 9 — KEY STREAM ANCHORED — 2026-10-23 00:35
- 6B keys [u16 delta][u32 smallest-three quat] CONFIRMED by
  perfect-stride repeats + 100%-valid phase-locked runs.
- rec874: @288 header (B=254) + 18B + 254 keys x 6B = ends
  EXACTLY at the @1830 section. @0 header (B=2) = root keys.
- Remaining unknown: @1830 section (B=201) layout + tail
  stream accounting. Method set: anchor-scan for runs ending
  at EOF/count boundaries.
- NEXT: Rust walker in anim.rs with these anchors as tests.


## ANIMATION HUNT 10 — WALKER IN RUST, TEST GREEN — 2026-10-23 00:45
- anim.rs: find_sections + read_rot_keys; tests/clip_real.rs
  locks clip a0's anchors (254 unit-quat keys, stream ends
  exactly at 1830). All 7 real-data suites green.
- Pair 2 (@1830) = translation side: reproduces tracklist 9f's
  root translation (0.101, -0.1393); tail holds a 6B
  [packed-vec3][delta 1-2] translation key stream.
- OPEN: @1830 accounting; packed-vec3 layout; rotation-key
  distribution over tracks (row counts sum 343 vs 254 keys).
- NEXT: split keys per track; apply header-875 remap; render
  a posed frame of clip a0 on Kaileena.


## ANIMATION HUNT 11 — FIRST PLAYBACK RENDER — 2026-10-23 01:00
- Clip a0's rotation stream = ONE track: 253 keys, delta sum
  425 frames ~ 14.2s @30fps ≈ descriptor f32 15.0 (duration s).
- pose_test --render clip<K>b<B>: key 120 on L UpperArm
  (bone_idx 9) = clean natural pose, left arm raised high —
  first real animation frame rendered from clip data.
  (c120_b9.png; Spine1 pitches wrongly, Head breaks.)
- Header 875 entry 16 -> v=10 (1-based) = bone_idx 9: matches!
  if this stream is the table's track 16. Directory-row ->
  channel -> header-entry chain is the last formal link.
- NEXT: segment the tail streams; assign all tracks; render
  the full multi-track pose at several frames.


## ANIMATION HUNT 12 — FULL WALK EOF-EXACT; REMAP SUSPECT — 2026-10-23 01:15
- Clip a0: 43 sections walk EXACTLY to EOF. ROT = 18B hdr +
  count x 6B; TRANS = 18B hdr + count x 14B ([delta][f32x3]);
  lead byte2 = channel 0..33; trans on channels 0,1,2,6,13,
  23,33 only. 4,324 rot + 512 trans keys total.
- Channel 0's first trans key == tracklist 9f's root
  translation (0.101,-0.1393,-3.5901) — ch0 = root channel.
- Full pose via header-875 table FAILS (streaks, h=5.86):
  the 42-entry tables likely belong to a 42-bone rig (the
  GUARD!) or index differently. 
- NEXT: empirical assignment — each mapped channel's stream
  alone x each bone, rank by locality + human-scale AABB.


## ANIMATION HUNT 13 — FULL POSE VERIFIED (ROTATIONS) — 2026-10-23 01:35
- Header-875 channel->bone table is CORRECT for rotations;
  the streak was my error: translation streams are scene/
  placement-frame values, not bone locals (ch0 = root
  placement). Rotations-only full pose at frame 200 = clean
  dramatic cinematic pose (anim200_rot.png); frame 0
  coherent; frame 400 tangles (late-frame defect, next bug).
- M5 animation is effectively DONE for one clip: descriptors,
  headers, sections, rot+trans streams, remap, skinning —
  all verified on real data.
- NEXT: fix frame-400-class defects; translations' role;
  clips a7/a2; guard/soldier; Bevy playback. Then M6.


## ANIMATION HUNT 14 — LATE-FRAME DIAGNOSIS — 2026-10-23 01:45
- Tangles at f300/400 = cloth/hair slabs sweeping unsimulated
  + two large late swings in mapped channels (ch0->Spine1
  99 deg, ch33->L Thigh 130 deg, frames 200-400). Body core
  coherent at all sampled frames. Genuine choreography vs
  residual misassignment needs ground truth — flagged open.
- Translation streams: ch0 = root placement (verified match
  with tracklist 9f root); other trans channels (1,2,6,13,
  23,33) role TBD — likely object placements in the scene.
- M5 status: COMPLETE for clip a0 with the caveats above.
  Next major: M6 gameplay core, or a7/a2 + guard/soldier
  animation, per user's direction.


## ANIMATION SET — KAILEENA COMPLETE; NPC BINDING OPEN — 2026-10-23 02:45
- New formats: Rot5 (5B keys) + Trans13 (13B keys) + clip
  Footer sections. All 5 Quai clips walk EOF-exact, zero
  unknowns (5 clip tests green).
- Kaileena: a0/a2 (hdr 419) + a7 (hdr 313) render; tracklist
  9f via hdr 313 == a0 frame 0 BYTE-IDENTICAL render; a1
  renders extreme pose. Her set is DONE (cloth caveat aside).
- NPC 6d family: rigs = BabylonSoldier + HG guard; soldier
  bind verified. Identity + name-translated maps FALSIFIED
  for 6d46. NPC map source = next dig (Technical Bank /
  6d bank records 508-516).
- M5 fully closed for the heroine; NPC binding is a
  standalone RE item. Next major per roadmap: M6 gameplay.


## NPC BINDING DIG — NARROWED — 2026-10-23 03:15
- 6d bank duplicated per level; descriptors identical
  (header always none); 508-516 = alias stubs.
- Soldier's own header 0x420031eb exists but is ALL-ff:
  the table is a per-character override, empty for NPCs.
  Identity map falsified on guard too.
- NEXT DIG: decode clip DIRECTORY rows (a0: 15 rows/34 ch;
  6d46: 22 rows/~35 ch) — the last undecoded clip structure
  and the likely binding source for header-less clips.


## CORRECTION — NPC BINDING SOLVED (IDENTITY) — 2026-10-23 03:35
- The "identity falsified" call was my error: 6d46's Rot5
  streams span ~25-40 frames; renders at f100-400 all held
  the final key. Within span, identity map renders a
  coherent falling/death clip on BOTH soldier and guard.
- NPC binding = channel i -> bone i (record order), no
  table — hence all-ff NPC headers + header-less
  descriptors. Kaileena's override table is the exception.
- 6d46 has no directory section (its section 2 is a Trans13
  root stream); a0's path section is a camera/root path,
  not binding.
- ANIMATION SET COMPLETE for all Quai characters.


## M5 PLAYBACK IN THE BEVY APP — 2026-10-23 04:10
- src/skin.rs: Character/Clip/ChannelMap/posed_verts (the
  verified pose_test pipeline, now library code).
- tests/playback_real.rs: parity vs pose_test renders
  (Kaileena a0@200 header map; soldier 6d46@10 identity) —
  exact. Full suite 13 green.
- t2t_game: Kaileena animated in Quai (clip a0, 30 fps loop,
  per-frame re-skin). Builds clean; headless VM stops at
  winit (no DISPLAY / libxkbcommon) — runs on his PC.
- M5 is now COMPLETE end to end. Next: M6 gameplay core.


## M6 FOUNDATION — 2026-10-23 11:00
- src/collision.rs: triangle soup (Quai 56,686 tris) +
  ground-height queries. src/controller.rs: gravity /
  ground-snap / jump character controller.
- Kaileena spawns at her REAL level spot (the FAKE_Kaileena
  stand-in's location; floor z=61.95), not the world center.
- t2t_game is now controllable: WASD (camera-relative),
  Shift run, Space jump, mouse-drag/arrow orbit camera.
- Verified: tests/m6_real.rs (drop lands exactly on the
  floor; walk-off-edge falls). Full suite 14 green; Bevy
  release build clean (one service restart mid-build —
  cargo resumed, nothing lost).
- M6 continues: wall collision, traversal/climbing,
  locomotion clips (Prince survey), combat, enemies/AI.


## M6.2 + PRINCE + REAL CAPTURE — 2026-10-23 11:20
- Wall collision shipped (resolve_walls face+edge rules;
  head-on stop and slide verified; spawn undisturbed).
- The Prince found: body GEO 0x510022d3 (M_PrinceBare,
  964 pts / 45 bones), rig = B_Pr_ family (Bip + Face +
  Hair, 83 GAOs, prefix "B_Pr"), Dark Prince body
  0x6d0025b7 (M_SandPrince_Body). Bind render coherent
  (out/prince_bind.png); bind anomaly bones 10+ (~0.95)
  documented in FORMATS.md as an open item.
- EVIDENCE: t2t_game now RUNS on this VM (Xvfb + Lavapipe
  Vulkan via ~/mesa-vk) and was captured with ffmpeg
  x11grab — screenshot + 10 s video in t2t-bevy/out/
  evidence/. Camera framing is now relative to character
  height; lighting brightened (dir 12000 + ambient). The
  world still renders with per-element average colors
  (dark) — real texture binding in the Bevy mesh is a
  known gap for a future polish pass.


## PRINCE PLAYABLE — 2026-10-23 11:45
- Bind "anomaly" resolved: library pipeline is exact for
  his body (prince_diag); the scare was a pose_test
  indexing artifact. Hair/sword binds permuted (cosmetic).
- Clothed body GEO 0x6d002617 (M_Prince_Body) lives only
  in PrinceLoading_Shape with a posed rig — bind-rig
  source still open.
- PrinceFinal_Shape's anim record 0x42004cd3 is a
  TRACKLIST (single leap pose, 21 channels), not a clip —
  he renders/plays it coherently under the identity map.
  His real locomotion bank is the next hunt.
- t2t_game presets: Quai=Kaileena, PrinceFinal=Prince.
  Evidence captured live (out/evidence/prince_game_*):
  the Prince in-game under physics + follow camera. The
  Shape sector is a sparse diorama (2,547 tris) — a real
  gameplay sector is the better home once his locomotion
  bank is found.


## ANIMATION BANKS + STATE-DRIVEN PRINCE — 2026-10-23 12:05
- Bank map charted (166 anim sectors): AMIR_wow is the
  Prince's actor sector (rig + 72 traversal clips, shared
  with the garden sectors); ACT_Enemy_* per enemy;
  Char_wow common acrobatics; area sectors hold
  cinematics (one 324-frame clip found).
- Cross-sector playback verified with rendered strips:
  ledge-hang, roll, dive, hang-idle, pull-up, aerial flip
  — all coherent on his rig under the identity map.
- In-game: the Prince now dives — airborne plays the AMIR
  dive clip, grounded the leap tracklist; spawn lift
  raised so the dive shows at spawn and on every jump.
- Still open: the base stand/walk/run bank (not in
  AMIR/Char) — next hunt via descriptor name tables.


## LOCOMOTION HUNT — VERDICT — 2026-10-23 12:25
- Descriptor format fully decoded (full key = high half
  at payload+4, low half in the type word); data records
  are gro 0x20025, same grammar as 0x2002b clips.
- The Prince's complete descriptor set (92 anims, from
  AMIR + garden sectors) is resolved, scored and sampled:
  traversal, combat reactions, falls, dives — NO base
  wait/walk/run. Those are referenced from his action
  table (AMIR logic records) — the next RE frontier.
- In-game state now: grounded = leap tracklist, airborne
  = AMIR dive (verified in live capture: he spawns high,
  dives prone, lands into the leap pose).


## ACTION TABLE XREF — 2026-10-23 12:55
- AMIR's logic records reference clips directly (1-2 per
  action) — the same traversal/action set + 21 contextual
  extras, all test-played. Gameplay-sector logic holds no
  clip refs. Base wait/walk/run is bound by a mechanism
  with no plain key refs in wow data — the action-record
  structure itself is the next decode target.
- Game state unchanged (grounded leap / airborne dive) —
  no truthful locomotion cycle exists yet to wire.


## ORPHAN + LOOP-CLOSURE SWEEP — 2026-10-23 13:50
- 300 orphan clips mapped: enemy sets, creature cycles
  (the Char bank's perfect loops are the chariot horse),
  action extras. No Prince locomotion.
- AMIR's action records = extended descriptors (clip key
  at fixed slot + parameter words) — format progress for
  the future GAM decode.
- Four methods now agree there is no key-referenced
  ground locomotion cycle. Hunt closed; remaining paths:
  GAM parameter decode / exe tables (deep RE), or pivot
  to traversal gameplay with the verified clip set.


## TRAVERSAL LIVE — M6.3 — 2026-10-23 14:15
Ledge-grab gameplay works end to end: find_ledge +
controller HANG state (grab / shimmy / climb-up / drop),
proven on a synthetic platform and on the real Quai
geometry (suite 17 green). The Prince now guests in
Quai (T2T_CHAR=prince): staged off the dock's east edge,
he fell, caught the rim, and held the hang pose —
screenshot + video in t2t-bevy/out/evidence/.
Controls in hang: A/D shimmy, Space climb up, S drop.


## BEAM MECHANICS — M6.4 — 2026-10-23 14:45
Beam-walk mechanics done and tested (width detection +
centerline lock; synthetic beam + a real Quai plank
anchor). Survey correction: the Beam/Column banks are
grip traversals (under-beam shimmy, side-hangs, pole
spin) — no standing beam-walk clip exists there, so no
clip was wired rather than fake it; the state plays the
grounded placeholder until the base locomotion gap
closes. Column spin clip identified (0x4e001797) for a
future pole state.
