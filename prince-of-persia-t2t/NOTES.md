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

## Addendum 2026-10-23 (combat slice)
Combat shipped in t2t_game: src/combat.rs (Fighter/StrikeProfile, 3 unit tests); the Prince (F / right-mouse) swings the SpeedKill spin slash 0x4e001719, reels with AMIR knockback 0x970005d0; the Sand Garde (own rig + bank, guest-loaded) approaches on the controller, pounces (0x6c000506), takes knockdown-rise hits (0xa20008ca), dies with 0xa2000ac3; HUD shows both HP; player death respawns both. Verified live on this VM: enemy spawns on the Prince's floor 242 units away in Quai, fight loop ran to two Prince deaths + respawns in one 38 s capture (out/evidence/prince_fight_video.mp4 + prince_fight_screenshot.png). Placement lesson: assembled sectors have sparse walkable floors — placement must search from the player's LANDING floor (anchor fallback gz is the AABB top). Player-side hits on the enemy are unit-tested (no key injection on this VM to demo his swing live).

## Addendum 2026-10-23 (pole state)
Pole traversal shipped: collision::find_pole (ray-ring detector, see FORMATS.md), controller PoleState (grab/climb/orbit/leap/step-off), t2t_game wiring with the verified Column-bank clips (grip 0x4e00178a, climb 0x4e001784, spin 0x4e001796) at animation priority below combat, above hang. Two detector designs were falsified on real data before the ray approach worked: centroid clustering (real meshes too coarse) and a 5-of-8 ring threshold (face points reach 5 via corner hits). Sector census found no climbable column in Quai / Central Garden / Inner Garden / Tower P01 — pole ships synthetic-verified (tests/pole_real.rs, 2 tests) and will engage wherever a real column exists.

## Addendum 2026-10-23 (second enemy: Human Guard; multi-enemy roster)
The Human Guard joined the Quai fight. His bank (ACT_Enemy_HumanGarde) is self-contained (rig + clips) like the Sand Garde's, and the shared pool clips play coherently on B_HG (pose-matched against the SG reference strips). t2t_game now runs a roster: Vec<EnemySpec>/EnemyChar+idx, per-enemy placement with 180-unit separation, per-enemy AI in the one system, HUD listing all pools, whole-roster reset on player death. Live capture: duo fight in Quai, three Prince deaths in a run (out/evidence/prince_fight_duo_*). Soldier caveat stands: pool contorts on his rig — a soldier enemy needs his own 6d-family set surveyed first.

## Addendum 2026-10-23 (Babylonian Soldier via name-based channel map)
The soldier blocker was a bone-ORDER difference, not a format wall: same Bip names, different GAO order (arms before legs; no skirt bones). ChannelMap::from_bone_names (skin.rs) pairs clip channels to bones by normalized Bip name — 34/46 channels pair — and the shared pool then plays on B_BabylonSoldier pose-for-pose. He joins the Quai roster (rig from 100_Quai, clips from ACT_Enemy_SandGarde, map_from the SG rig). Live trio capture: 5 Prince deaths in one run (out/evidence/prince_fight_trio_*). The same trick should generalize to other Bip-ordered rigs (archers, Thrall, etc. — 17 ACT_Enemy banks exist).

## Addendum 2026-10-23 (ALL banks resolved from the name map)
Per the user's directive, the name-map resolution ran over every ACT_Enemy bank (resolve_banks census, strips in t2t-bevy/out/resolve/). Resolved + rostered: SandGateGarde (42/46), Thrall (39), DarkWarrior (36), SandArcher (35, body GEO — his second skinned GEO is the bow), HumanArcher (35), both Twins bodies (34 each), Enchantress (31) — joining SG, HG and the Soldier for an 11-enemy roster, selectable via T2T_ENEMIES (default: original trio). Not resolved: Mahasti (33 pairs but the gown slabs — own-clip route next), Chameleon/DarkReptus/Hunter (creature rigs, 22 pairs), Illusion (effect mesh, no clips), Rowan (rig not in bank), Pigeon (bird). Placement guard added: unplaceable enemies are dropped with a log line, never spawned at the origin.

## Addendum 2026-10-23 (Mahasti resolved via her own clips)
The "gown slab" was a red herring: Mahasti's name-map pairings were all anatomically correct — the pool's diving lunge swings her unanimated loincloth bones as a rigid panel. Her bank holds a dedicated 0x6603xxxx moveset (29 clips, 35 channels) that binds by IDENTITY on B_KA. Rostered with cycle a66a / spin-attack a66d / hit a649 / death a3f6 (forward collapse; no prone death in bank). Roster is now 12. Survey tool: src/bin/mahasti.rs; strips out/mahasti_*_identity.png.

## Addendum 2026-10-23 (sand system + slow motion)
Sand economy is in: SandTank in combat.rs (cap 6, +2 per kill credited once per death), R key spends 1 tank on 4 s of slow motion via per-actor dt scaling in animate_char (enemies 0.3x, Prince 0.9x, camera real-time). HUD shows the tank and a live slow-mo countdown; death resets the tank. Verified headless with staging envs T2T_SAND/T2T_SLOWMO_AT (log line + HUD countdown strip 4.0→0.2 s in out/evidence/prince_sand_slowmo_*). Suite 30 green.

## Addendum 2026-10-23 (checkpoints + fall respawn)
Respawn is now progress-based: a Checkpoint tracker (controller.rs) re-bases to any grounded spot 400+ units past the current one; deaths respawn there (yaw restored), and falling below the world (kill_z = AABB min - 400) is fatal through the same path. Verified live twice: checkpoint-seeded deaths x6 at the seeded dock spot; void-spawn fall recovered to the checkpoint and the fight proceeded. Suite 33 green. M6 is now feature-complete (traversal, pole, combat, 12-enemy roster, sand, checkpoints); remaining big rocks: GAM/action decode, clothed-body bind rig, M7 presentation.

## Addendum 2026-10-23 (animation bank found; locomotion hunt II)
The binding graph is solved to the bank level: AMIR record 0x2d000acc = the Prince's 183-slot animation bank (all descriptors, ordered); behaviors reference descriptors by key and pick bank indices from compiled bytecode (movement logic + debug strings in record 0xb40001a2). Exe/DLL raw-key tables: none. Stride census (new) over all 651 clips: no upright gait cycle for the Prince exists in the archive; lookalikes were Rowan/Hunter clips and traversal moves. Locomotion verdict stands, now six methods deep; the bank table itself (out/anim_bank.txt) is a permanent asset. Remaining cracks: behavior bytecode disassembly; Rowan's 0-bone skin GEO.

## Addendum 2026-10-23 (clothed Prince: recovered statue, not playable)
reconstruct_bind() (skin.rs + gao.rs mat_inv) recovers the clothed Prince's posed skeleton from its GEO bind matrices EXACTLY (RMS 0.0, 39/39). The loading mesh is a collapsed statue whose per-bone bind geometry matches the canonical rig under no rigid frame transform (searched translations/rotations/mirror; best mismatch 3.2), so canonical clips cannot play on it without per-bone axis correction — future research, documented in FORMATS.md. Game keeps the bare Prince body.

## Addendum 2026-10-23 (GAM anatomy session)
Bank record layout mapped (refs +0..+2360 of 5,364 B, grouped); descriptor anatomy extended (f32 at +34 = rate-class param, variable small-int tails from +52); behavior records are Jade VM bytecode with trailing string pools (movement = 4 functions in/near record 0xb40001a2; 0x21 opcode pattern). Locomotion remains bound inside the bytecode — a disassembler is the only route left and is a multi-session undertaking. Tools: bank_dump, bhv_dump.

## Addendum 2026-10-23 (disassembler v1)
Jade behavior records cracked at the structural level: header/pool/cell-stream split, (id,type) cell grammar with float-bit cells, function-id map (68 records; movement functions located: 0x5022 movement+coef record, 0x4eca TransitionToMovement, 0x504f TimeToTravel). jade_dis.rs prints annotated listings. Remaining: node grammar, the movement function's action values, and the action->animation mapping.

## Addendum 2026-10-24 ~00:20 IST — M7.1 menu shell DONE
t2t_game now boots into a main-menu phase: the game's real Menu3D scene (84,887 tris) as backdrop with a slow orbit camera and title UI (PRINCE OF PERSIA / THE TWO THRONES / Press ENTER to start / controls line); Enter transitions into gameplay (visibility swap, camera hands over to the follow cam). Implementation: Phase resource (Menu/Playing) + run_if gates (animate_char only in Playing), MenuEntity/GameEntity markers, Menu3D prepared separately unless the sector IS Menu3D. Staging: T2T_START=play skips the menu (all older capture flows keep working); T2T_MENU_SECS=<s> auto-starts for headless evidence. Menu framing is data-driven (new bin menu_probe): Menu3D mass sits at Bevy y ~-4.8, XZ centroid (-0.5, 1.0) — the orbit (r=10, y=-2.2, look (-0.5,-3.0,1.0)) sits inside the skybox ring above the island; two earlier framings failed honestly (outside the ring = panorama panel occludes all; origin-height = island glued to frame bottom). Verified headless: out/evidence/prince_menu_transition_video.mp4 + stills (menu city view; post-transition Quai/Kaileena, UI gone). Suite still 34 green. REMAINING in M7: DARE audio decode (sound.big/.fat), Bink video, real texture binding for world meshes (the dark-scene gap).

## Addendum 2026-10-24 ~00:45 IST — DARE audio recon
FAT format cracked exactly (see t2t-bevy/docs/FORMATS.md) + dare_tool (list/extract, byte-verified). gamesnd.sp0 = master sound database (1,113 sound ids). .sb0 descriptor schema still open; data region decodes plausibly as IMA ADPCM (~3.6% saturation — provisional) with test WAVs written for a listening check.

## Addendum 2026-10-24 ~01:05 IST — DARE audio FULLY CRACKED
Per the user's direction, PopAudioStudio v1.3 (kaminoer) was disassembled and its decoder logic reconstructed into out/dare/pas_decoder_spec.md, then implemented as src/dare.rs + dare_tool decode/census. The format (see FORMATS.md): FAT index, .sb0 banks (TableA/TableB/regionC/chunks), primer-IMA sections (28-byte header + 10 raw primer frames + standard IMA, high nibble first) or raw PCM, literal per-sound Hz rates, and STREAM.SS0/STREAM.LS0 link records (bank 000 = 716-link streaming catalog). Census: all 162 banks, 2,342 sounds, ~369M frames, 0 failures. The earlier "IMA close but saturating" decode failed only because it lacked the primer/seeds and used low-nibble-first. NOTE: an unverified tool output mid-session fabricated a Rust/CLI/sb0-index string family for the tool — discarded after byte-level checks against v1.0-v1.3 binaries (the tool is MSVC C++).

## Note 2026-10-24 ~01:16 IST — gamesnd.sp0 carries the semantic sound layer (combat-foley dig, parked)
gamesnd.sp0 (330,356 B in sound.big @ 14,113,024): hex-id records at a 48 B stride from ~1864 (ids 0x03/0x05/0x06/0x08/0x09/0x0a families — the actual SOUND ids; the TSV's 0x17xxxx are Jade object/event ids, a different space bridged only by AI scripts). Deeper in the file (~55k+) sit the sound-design SOURCE paths in a strided/wide encoding (fragments: "Y:\sound003...Spanish\99002...Transform_S00P.wav/.ima", "MONO_Prin...urt_05.w..." = Prince_Hurt_05.wav, language rows English/French/German at 24 B stride). So event->filename->bank-sound resolution is feasible: parse the wide strings, join hex ids to bank records (bank 000's TableA region carries the same hex ids as names). This is the route to grounded combat foley (swing/hit/hurt/death) — do NOT pin arbitrary foley before this dig.

## Addendum 2026-10-24 ~02:15 IST — M7 audio + Bink intro WIRED & VERIFIED
AUDIO: t2t_game decodes DARE at startup (prepare_audio): menu music = stream event 0x700000 (64.0 s stereo 24 kHz, MenuInGameAI/Menu_Boot-referenced) looped at -6 dB during the menu phase; UI boot 0x190002 at menu start, confirm 0x190007 on start-game (bank 002 = the Menu_Boot bank, via its TableA event map — now parsed into Bank.events; BankSound gained the TableB id). Playback via bevy_audio AudioPlayer; music entity despawned on transition. TWO bugs found by headless runs and fixed: (1) Bevy's default features compile NO wav decoder — rodio Decoder::new().unwrap() panics on first playback; fix = bevy feature "wav" in Cargo.toml. (2) systems must take Option<Res<...>> for resources that staging can omit (IntroState) — a hard ResMut panics when absent. Verified headless (ALSA null device): full sequence audio ready -> music+boot -> menu->playing -> music stopped + confirm, 0 panics (out/evidence/audio_menu_run3.log).
INTRO: all Video/*.bik are Bink 1 (BIKi, 800x600 @ 29.97, binkaudio_dct) — ffmpeg decodes them natively. Boot sequence UBISOFT -> UBIMC -> LOGORP (the title/legal card) -> GO_GEN pre-extracted to out/video_cache (549 JPEG frames, 17 MB + per-clip WAVs; cache dir override T2T_VIDEO_CACHE; staged off by T2T_NO_INTRO=1 or T2T_START=play). prepare_intro + intro_update: fullscreen ImageNode streams frames into one shared Image asset, clip audio via DESPAWN AudioPlayer, Enter/Space/click skips a clip, menu audio starts at the handover. Verified headless: all 4 clips advanced, handover logged, stills show the LOGORP title card and GO_GEN golden-vine clip in the game window (out/evidence/intro_run.mp4 + intro_t*.png, logs intro_run.log/intro_run2.log). Combat foley still pending the gamesnd filename dig (parked note above).

## Note 2026-10-24 ~10:45 IST — gamesnd dig: chain mapped, middle link missing (combat foley stays parked)
gamesnd.sp0 layout: [0x00150000][0x20]["BANK"] header, channel defs ("Default Multitrack Channel"), then 1,113 id records x48 B = [8 B zeros][u32 ptr][u32 kind][32 B name "0x........"] starting at file offset 1848; ptr values start 0xd6b4 and stride +0x18 into a node-graph region (~54,964..330,356) of 24 B nodes in the same (id,type) cell grammar as Jade behavior records — file paths are stored as string chunks across nodes (fragments in file order lose chunk heads, so naive joining yields "nsform_S003_P.wav" for Transform_S00P.wav, "ONO_Prin...urt_05.w" for MONO_Prince_Hurt_05.wav; other recoverable names: prince_wait / prince_rollstone / prince_stepgrid ambience, VO_* scream lists, MU_400_C0xx_Faile*.ima music cues, language rows English/French/German at 24 B stride ~56,936). BRIDGE REALITY CHECK: only bank 000's TableA carries hex-id names (its streaming catalog is gamesnd-driven); level banks (002/003/005...) carry NO names — their sounds are anonymous TableB records. The combat chain Jade event (0x17xxxx) -> gamesnd id -> bank sound is missing its FIRST link (it lives in AI scripts / DARE runtime tables, i.e. Jade VM territory). Honest options: (a) write the full sp0 graph parser (multi-hour; names for streams + maybe event links), (b) find the event->id table in pop3.bf DARE records, (c) leave foley parked and do texture binding next. NO foley wired — wiring anonymous bank-003 sounds to sword events would be faking it.

## Addendum 2026-10-24 ~11:00 IST — event->sound table FOUND in the banks; sword blockSound wired
The hunt resolved INSIDE the .sb0 banks (not pop3.bf): a bank's TableA is the sector's sound-event catalog: (event id, type=5, TableB index) — bank 003 alone catalogs 141 events of the 0x17xxxx gameplay family. TableB record kinds: type 1 = wave; **type 4 = CONTAINER** (child count @+0x1c, region-C offset @+0x0c -> 16-byte entries {u32 child TableB index, u32 weight (siblings sum to 65536), 0, 0} — weighted variants of one sound); types 3/7 also carry region-C offsets; type 8 = localized-voice links (0x86xxxx). Region C thus holds names AND container child tables. Resolution chain (all in dare.rs now: Bank.events + Bank.containers + resolve_event): event -> TableA -> TableB wave or container children. PROOF: event 0x170024 — the blockSound referenced by every ACT_Weapon_* object (46 TSV rows) and by Enemies_Weapon::fn_PlayBlockImpact in Prince_Weapons (record 0x3207cbb5; its sibling asserts name Impact_Block / Impact_Hurt / ground-wall impact) — resolves in bank 003 to container TableB 26 -> waves 27/28/29 (0x17012f/0x170131/0x170165; ~0.3 s @ 18 kHz metallic impacts). WIRED in t2t_game: any strike_connects() hit (either direction) plays a random variant, log-tagged. The 0x17 catalog also covers the Prince's Amir AI events (170097/1700CA/1700D6/1700EB/1700EC/1701A5/1701C8) — semantics unproven, not wired. snd_hunt tool (src/bin/snd_hunt.rs) did the pop3.bf sweep: sound sectors are per-level *_Sound_wow files keyed by gamesnd ids holding trigger/actor GAOs; no event table lives in pop3.bf itself.

## Addendum 2026-10-24 ~11:20 IST — container-count correction: the first foley build shipped a silent bug
The first wired build never played a block sound in game, though the fight itself ran (screenshot foley_fight5_t45.png: Prince HP 3/5 mid-pounce on the dock — enemy strikes were connecting). Root cause found by probe (src/bin/block_probe.rs, kept as a tool): Bank::resolve_event(0x170024) returned EMPTY because parse_bank read the container child count at +0x24 — that field is always 0, so every container parsed with zero children. Byte-level check of all 27 type-4 records in bank 003: the count is at **+0x1c** (tb26: 3) and child entries are 16 bytes {child, weight, 0, 0} with weights summing to 65536 (tb26: 21864+21836+21836). Fix in dare.rs (count @+0x1c, stride 16); FORMATS.md corrected. After the fix block_probe resolves 0x170024 -> 3 waves (6185/4929/5287 frames @ 18000 Hz, matching the exported block_variant WAVs). NOTE for future verification: the "falls - respawning at checkpoint" log line fires for ANY Prince death (fall OR combat death) — it does not prove a fall; check HP/screenshot to tell them apart. Also: T2T_CHAR=prince in Quai spawns in the sky (311) because the Prince preset's anchor falls back to the AABB center; the seeded dock checkpoint (-405,-146,65) is the working fight staging.

## Addendum 2026-10-24 ~12:50 IST — world texture binding DONE (world only)
prepare_world used to flatten the whole sector into one mesh with per-corner colours = each element's texture AVERAGE. Now it splits faces by resolved texture key (t2t_game WorldMeshGroup) and setup builds one Bevy mesh + StandardMaterial per group with the real decoded texture (decode_texture: DXT1/3/5 + BGRA + PAL8-greyscale; Repeat wrapping; textures mixing opaque + transparent pixels get AlphaMode::Mask(0.5) for cutouts). Census (src/bin/tex_census.rs, kept): Quai 55,322/56,686 faces textured, 70 keys, 9.0 MiB RGBA, 0 undecodable; Menu3D 100% textured, 53 keys. Game log: Quai 71 groups / 70 decoded / 20 cutout; Menu3D 53/53/25. Verified headless: same dock-fight framing before (foley_fight5_t45.png, flat blobs) vs after (texbind_t60.png, plank/striped props) — real sampling, not flat colour; 0 panics; foley still firing (25 impacts in the texbind run). Menu backdrop at orbit range shows magnified low-res island textures (expected: 128px textures, close camera), not a rendering failure. NOT done: characters/enemies still render flat-tinted — Character::load drops the GEO element matIds and has no grm chain, so their texture keys are unresolvable without a loader change. Also open: mipmaps (minification shimmer at distance), PAL8 palettes (currently greyscale), UV-orientation proof (as-is matches D3D top-row-first convention; no inverted patterns observed).

## Addendum 2026-10-24 ~13:10 IST — character texture binding DONE (atlas route)
The character loader dropped GEO element matIds, so the chain had to be rebuilt in t2t_game: char_textures() re-parses the rig sector's GEO, finds the owning GAO (gro_key == geo, or a group record naming it) for the grm chain, and resolves per-element texture keys via texture_key_for. KEY FORMAT FINDING: every character texture key appears TWICE in its sector — a 56-byte header-only stub FIRST, then the full record (65592 B for 256x256 PAL8). First-match lookups decode EMPTY; last occurrence wins (same as assemble's RecordIndex overwrite). The world path was already correct for this reason. Skins stack into one atlas (single column, key order) with UVs remapped face-major — the one-mesh animation path is untouched. Verified: Prince 3 DXT5 keys -> 512x1088 full-colour atlas (torso/face/tattoo atlas confirmed by eye); Sand Garde / Human Guard / Babylonian Soldier 1 PAL8 key each -> 256x256. Fight screenshot before (flat beige blobs) vs after (skin, cloth, trim detail), 0 panics, foley still firing (25 impacts). OPEN: PAL8 palettes. The full records hold exactly w*h index bytes — no palette room — and no palette key appears in the grm/sub-material payloads. A quick sector scan found 1024-byte records (e.g. 0x510001e9, 252/256 alpha-255; 0x510041fd, 256/256) that could be palettes, but no link from texture to palette is established. Until that is cracked, PAL8 skins (Kaileena, all enemies) render as greyscale detail — correct shapes, wrong colours. Kept probes: char_tex_probe.rs (fixed to last-wins), tex_dump.rs, tex_png.rs, key_hunt.rs, pal_hunt.rs, grm_dump.rs.

## Addendum 2026-10-24 ~13:40 IST — real menus/HUD from the original MENU data (M7.2, verification pending)
Menu TEXT source found: POPData.bf (22 KB, beside pop3.bf) holds pop4pclocal.dat — a plain INI localization file (1011 lines: P_MainMenu, P_InGameMenu, P_Options, P_SoundsMenu, P_VideoMenu, P_ControlsMenu, P_GameOptions, P_GameOver, ConfirmQuit, MapNames, Actions, Keys...). The container's name table is only partially reverse-engineered (two entries; the .dat is the last file and runs to EOF), so load_menu_strings() locates it by its first line "; General information" and validates on "[P_MainMenu]" — with identical English fallbacks if POPData.bf is absent. The .MGB/MAGMA menu banks (P3MAIN/P3INGAME/P3COMMON + EU/ per-level banks) were NOT format-cracked: they carry texture references (Frame/Thumb/Arrows/BoxBackground .tga), font names (POP3_24/POP3_54, matching FONTS/*.MFT), and group names, but no readable menu text — the structure comes from the .dat sections, not the MGB layout. t2t_game now has: MenuScreen/MenuState + stack navigation (Arrows/W/S, Enter, Esc), main menu (Start New Game / Load Saved Game / Options / Credits / Change Profile / Quit), Options tree (Graphics/Sounds/Controls/Game + Back), pause on Esc (Continue/Options/Quit to Main Menu), quit-confirm dialog, session reset on Start (player + all enemies to spawn, full HP/sand, checkpoint re-seeded), segmented bar HUD (5 red health + 6 gold sand segments; enemy HP stays text). Wired toggles: Music on/off (menu music spawn/despawn) and Interface on/off (HUD visibility). Everything else in Options is a faithful label with no effect yet; Credits shows a not-wired note (CREDITS.BIK not transcoded); Game Over screen not built (checkpoint respawn still handles death); Special Load dev menu omitted (MapNames kept for M8). Staging: T2T_MENU_SCREEN=<main|options|graphics|sounds|controls|game|profiles|load|credits|confirm>, T2T_START_PAUSED=1. Gotchas fixed during this pass: Fighter.hp is i32 (bar comparison); Bevy function systems cap at 16 params — animate_char was at 17, so the HUD lives in its own hud_system; menu keys are gated while the boot intro is active (Enter belongs to intro skip).

## Addendum 2026-10-24 ~14:20 IST — M7.2 menus/HUD VERIFIED (after two fixes)
Verification (headless, 0 panics, lib tests 9/9): main menu (Start New Game / Load Saved Game / Options / Credits / Change Profile / Quit, gold selection), Options screen (Graphics/Sounds/Controls/Game/Back), pause overlay (Continue/Options/Quit to Main Menu/Back over the frozen scene), bar HUD (5 red health + 6 sand segments, enemy HP text). Loader proof: a test POPData.bf with "Start New Game" altered to "START-TEST-OK!" rendered that string as the first item — the labels genuinely come from pop4pclocal.dat, not the fallbacks. Two bugs found by verification, both fixed: (1) Bevy B0001 startup panic — two &mut queries over the same component in one system (Text for menu/pause info; BackgroundColor/Visibility in the HUD) — merged into single Or-filtered / kind-field queries; (2) the menu list stayed empty — a childless container has NO Children component, so the (Entity, &Children) list query never matched; the list entity is now queried without &Children. (3) Booting paused showed a blank frame — the boot-intro overlay spawned but never advanced outside the Menu phase; T2T_START_PAUSED now skips the intro like T2T_START=play. Note: bars in a paused-boot shot stay unfilled because no Playing frame ever ran hud_system — in real Esc-from-gameplay use they are filled, as the hud_bars shot proves. Still open: options rows beyond Music/Interface are labels only; Credits video, Game Over screen, Special Load dev menu unwired.

## Addendum 2026-10-24 ~14:40 IST — PAL8 palettes CRACKED + wired
Link: the 4-byte prefix at offset 52 of a format-1 texture record, read LE, is the key of its palette record (1024-byte non-texture record, 256 BGRA entries) in the SAME sector — toolkit `palette_for_texture` semantics, confirmed against our data by probe `pal_link` (all Quai PAL8s + Sand Garde in his own sector resolve; one orphan, 0x30014ad7 → 0x16005a43, has no same-sector record and stays greyscale — no first-palette fallback guessing). Render proof (`pal_render`): Kaileena 0x420035ae and Sand Garde 0x51001f23 in full plausible colour. Alpha rule (measured): palette alpha is literal only when some used index has alpha 255; Sand Garde / Human Guard palettes cap at 191, so literal alpha would ghost a solid enemy — decoder forces opaque in that case (`decode_texture_pal` in texture.rs, 4 new lib tests). Character atlases force opaque regardless; world textures keep the Mask/Opaque heuristic on the decoded alpha (Quai cutout groups 20 → 25 once PAL8 alpha went real). In-game verification: dock fight, 0 panics, all atlases resolved, foley firing, fighters now show gold/bronze armour + brown leather vs the old greyscale blobs (out/evidence/pal_fight_crop.png). Full lib suite 13/13.

## Addendum 2026-10-24 ~15:35 IST — Cine story clips VERIFIED
All 7 Video/Cine/P3C_*.bik are Bink 1, 640x448 @ 29.97, ~10 min total (010: 160 s, 020: 117 s, 030: 20 s, 040: 35 s, 050: 116 s, 060: 95 s, 070: 60 s), 5 untagged audio tracks each — first track used, language unverified. Extracted to out/video_cache/Cine/ (18,075 JPEG frames + per-clip WAV, 513 MB). Player: Phase::Cine + CineState/CineNode, same frame-streaming as the boot intro; gameplay systems are gated on Phase::Playing so Cine suspends the game, and on end the phase returns to return_to. Staging: T2T_CINE=<name|all>, T2T_CINE_AT=<secs> for delayed in-game interruption (no AT = play at boot). Clip audio stops on skip/end (tracked entity despawn — the intro does not do this). Bug found by verification: the first build's trigger never set Phase::Cine, so cine_update never ran and the black node sat over a running game forever — fixed by setting the phase in cine_trigger. Verified headless, 0 panics: mid-gameplay P3C_010 frame (medallion close-up), P3C_030 interruption resuming to gameplay (log "cine done -> Playing"), P3C_030 over menu returning to the menu (log "cine done -> Menu"). Skip is wired like the intro (Enter/Space/click) but not headless-verifiable (no key injection). Real story-beat triggers still belong to M8 campaign work.

## Addendum 2026-10-24 ~16:05 IST — Hurt foley WIRED, swing/death/ground left unwired
The three Enemies_Weapon impact functions (fn_PlayBlockImpact / fn_PlayHurtImpact / fn_PlayGroundAndWallImpact, record 0x3207cbb5) map to bank 003 events by elimination + waveform character: block 0x170024 (known, 3 variants), hurt 0x17000f (single wave, 0.38 s @ 20000 Hz, dull thud: fast-decay envelope, ZCR 0.054, centroid 220 Hz), ground/wall 0x170010 (single wave, 0.75 s @ 12000 Hz, low rumble swelling mid-sound, ZCR 0.020, centroid 117 Hz). Probes kept: src/bin/ev_resolve.rs (resolve events in a named bank or all banks; note: FAT order is NOT bank order, look banks up by name, and skip non-.sb0 entries — gamesnd.sp0 shares the bank magic and panics parse_bank), src/bin/wave_feat.rs (duration/envelope/ZCR/centroid), src/bin/str_hunt.rs (targeted string sweep). Strikes now play hurt (block variants stay loaded, reserved for a future parry state — this combat model has no blocking). Verified headless dock fight: 15 hurt lines, 0 block lines, 0 panics (out/evidence/foley_hurt1.log). NOT wired, with reasons: ground/wall — identified but no weapon-vs-wall detection exists to trigger it; swing — no clean lead (all "swing" strings are pole/chain traversal, "slash" hits are FX geometry names, and the Dynamic_Actions_Swing sector's co-occurring 0x17fa00/0x17fa03/0x170000/0x17000c events resolve in NO bank — COOCCUR alone is not proof); death — only Enemies_Spawner::fn_Notify_EnemyDeath exists (bookkeeping, not a sound player), and bank 003's Amir AI events (0x170097 etc.) remain semantics-unproven.

## Addendum 2026-10-24 ~16:15 IST — Credits video WIRED
CREDITS.BIK (Video/, Bink 1, 800x600 @ 29.97, 334 s, 1 audio track, 144 MB) extracted to out/video_cache/Credits/ (10,010 JPEG frames, 532 MB + 64 MB WAV). The Credits menu item now plays it through the Cine player instead of the placeholder screen: prepare_cine loads the credits clip from cache Credits/ whenever present (appended after any staged Cine clips, credits_idx remembered); a CreditsRequest marker resource (menu action, or T2T_PLAY_CREDITS staging env) makes cine_trigger play just that clip (stop_after) and return to Menu. Player changes for the 800x600 clip: cine_update swaps the node image asset on the first frame of a differently-sized clip (Cine clips are 640x448). Menu music stops when any video starts and restarts on return to Menu (intro handover guarded against double-starting it). Verified headless, 0 panics: short 60-frame fixture of real credits frames — playing CREDITS, music stopped, cine done -> Menu, music restarted, menu screenshot restored; full-cache run — credits roll frame (names/roles) on screen. Skip is wired like Cine (Enter/Space/click) but headless-unverifiable. The Credits screen placeholder remains only as the T2T_MENU_SCREEN=credits staging fallback.

## Addendum 2026-10-24 ~16:30 IST — Mipmaps DONE
World + character textures now upload with full CPU mip chains (texture.rs mip_chain: 2x2 box-filter levels down to 1x1, edge-clamped for odd sizes, levels concatenated with no padding as wgpu's create_texture_with_data expects) and Linear mipmap filtering on both samplers (Repeat for world, ClampToEdge for character atlases). Averaging is in sRGB space (standard cheap approximation). Bevy API note: Image::new asserts data == level-0 size, so mipmapped images are built with Image::new_uninit + texture_descriptor.mip_level_count set by hand (see t2t_game.rs mipmapped_image). Fullscreen video images stay single-level. Tests: 4 new lib tests (pixel average, 512x512 chain size, odd/atlas dims, level-0 verbatim) — suite 17 passed, 0 failed. Verified headless dock fight: 0 panics, 0 wgpu/texture errors, color character textures rendering, hurt foley still firing (out/evidence/mips_fight1.log). The distant-minification benefit itself (less shimmer) is expected from the technique, not measured from a single headless frame — no before/after delta is claimed.

## Addendum 2026-10-24 ~16:45 IST — Jade VM session: census + caller hunt
Slot-store census of the movement record is complete: slot-4 writes are {1,0,5,0,3,5,1,0,0,16} — values 2 and 4 are never stored there (closed with a negative). TransitionToMovement's tail holds a 1/2/3/0 case table, a different enum from the movement function's {0,1,3,5} modes; the bridge is still missing. Caller hunt: TransitionToMovement has NO bytecode caller in AMIR (engine-invoked, presumably); the movement-action function has exactly one caller, the "Prince total" behavior script (0x66038de7), which calls functions with operand 0 — so functions share state via var refs, not arguments. 0x0001 GAM siblings enumerated: 19 records (not 15), holding unbanked clips. Tools: slot_scan, call_scan, gam_scan. Full write-up in docs/FORMATS.md ("Jade VM — slot-store census + caller hunt"). Next: var-ref tracing.

## Addendum 2026-10-24 ~17:00 IST — Jade VM session: var-ref tracing
No action channel found. The three movement records share 19 var ids, but nearly all are machinery (closing idiom, engine-call idiom, assert groups). Best lead: every slot-4 store is followed by a (4, 21.500)(0, var:836)(0, var:834) reference — but var:836 is too frequent (195x in TransitionToMovement) to pin as one field. Correction: the pre-assert (0, 21.500)(2, 21.100) in TransitionToMovement is a store to slot 0, not a compare — the earlier "compare against 2" reading is withdrawn. var:900 operands are generic small ints everywhere, ruling out a numbered-setter shortcut. The Prince script never touches slot 4. Next route for this question is the exe's VM interpreter code, not more bytecode patterns. Full write-up in docs/FORMATS.md ("Jade VM — var-ref tracing").

## Addendum 2026-10-24 ~17:15 IST — Jade VM session: native counterparts in pop3.exe
UPX-unpacked a COPY of pop3.exe (game file untouched) and found the native behavior functions. Both movement functions agree: the action enum is {0,1,2} — the movement function returns a band index over 3 thresholds (ratio assert bound 1.0001, matching the bytecode exactly), and TransitionToMovement asserts action <= 2, stores it to actor field +0x5c0, and calls the transition machinery with old+new action. Consequence: the bytecode record's {0,1,3,5} slot-4 stores are NOT the movement action — that record is a larger behavior whose slot-4 state is something else (reinterpretation required; the earlier "returns action in slot 4" reading is withdrawn). The action->animation link enters through callees 0x4cc610/0x4adc80 (not yet decoded). Full write-up in docs/FORMATS.md ("Jade VM — native counterparts in pop3.exe").

## Addendum 2026-10-24 ~17:30 IST — Jade VM session: transition callees
Mechanism mapped, contents not: 0x4cc610 is a generic 2D table-index helper — TransitionToMovement looks up (old, new) action in a 4-byte-entry table inside the actor object (+0x1140); the entry (flags + state/animation id) goes through 0x4adc80/0x4adc50 into the animation state machine 0x8687d0, which strips flag bits, compares the id to the current state, and marks a transition when they differ. Which id each pair maps to is runtime data, not visible in the exe — its load source is the next question. Side note (unproven): the exe's type table carries tags 0x21/0x22/0x25/0x26 matching the bytecode operator ids. Full write-up in docs/FORMATS.md ("Jade VM — transition callees").

## Addendum 2026-10-24 ~17:45 IST — Jade VM session: transition table load source
Not determinable statically. The index helper has 1,092 callers (fully generic table system); a behavior function copies table entries between tables at load, but names no file or record; no bf record matches a plain table shape; POP3.dat is an opaque "SFFS" container, not a readable source. What would settle it is runtime inspection of actor+0x1140 after load — not available on this VM. Mechanism stands, load source stays open. Full write-up in docs/FORMATS.md ("Jade VM — transition table load source").

## Addendum 2026-10-24 ~17:15 IST — M8 step 1: campaign order + sector census
Campaign order = 66 MapNames SL entries from pop4pclocal.dat (SL_000 010_Deck through SL_065 710_MaskSanctuaryV03, alternate ending). Full wow census (tool: sector_census; log out/sector_census.log): 579 wow entries, 578 assembled, 83 main-sector candidates (placed>=50), 149 empty (actor/sound/manager), 346 small GFX/prop/link. World sectors carry area codes (100 Quai, 200 ThroneRoom, 620/630 Gardens, 800-820 Tower, 900 SPRoom/SOT). Key finding: SL names appear NOWHERE in entry names and the exe has no SL strings, and TimeLine.bin is an empty placeholder — so SL steps don't map 1:1 to sectors (five ThroneRoom visits share one sector); sectors are reused across steps with different states. The SL->sector map needs sector exit/portal trigger data or the save system's sector table — next M8 research step. Tools kept: sector_census, dump_raw.

## Addendum 2026-10-24 ~17:25 IST — M8 step 2: sector-transition graph
Portal graph built from in-sector GAOs (out/portal_graph.txt, 167 lines over 52 sectors): ~100 PORTAL edges covering the full area chain 100->110->200->300->400->410->420->500->510->520->600->610->620->630->640->700->710->720/730/740->800/810/820 plus the 900 SOT/WW chain; TEC_Sector codes (Se_area_num) show wow files hold multiple sub-sectors; ~30 checkpoint GAOs. No Deck/Beach/Ship sectors exist (opening SL steps are cinematic; Quai first playable). No SL binding found in saves (header-only profiles), BF level files (none by name), or exe strings (no wow names) — the binding is not a static table; next leads are portal GAO payloads and sector behaviour scripts. Tools kept: bf_names (full 6,648-entry list in out/all_entry_names.txt).

## Addendum 2026-10-24 ~17:35 IST — M8 step 3: portal payload archaeology (negative)
wow_rec tool kept. Portal GAOs are a uniform 198-byte template (name + matrix + trigger volume + flags + prev-key chain link) — no target key, no SL index; targets resolve by name fragment. TEC_Sector/CheckPoint GAOs share the template; checkpoint matrices are respawn positions. SL/wow name overlap is near-zero (only ThroneRoom, Garden) — SL names are a separate localization layer, and the SL->sector binding is not recoverable from static data found so far. Next M8 step that needs no SL map: make Depot (Quai->Depot->InnerCourt, the first verified portal chain) loadable/playable with portal travel.
