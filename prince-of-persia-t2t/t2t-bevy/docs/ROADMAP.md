# Roadmap — from archive reader to a 1:1 Two Thrones

"1:1" here means what OpenJade means by it: the same levels, geometry,
textures, animations, combat, enemies, audio and campaign flow, running
on a new engine that reads the original data. That's a staged project —
each milestone below ends in something runnable and verifiable.

## M1 — Archive layer ✅ (2026-10-22)
BIG reader + `bf_ls` + Bevy boot/index. Verified against the real
`pop3.bf` (6,648 entries).

## M2 — Stream decompression + inventory ✅ (2026-10-22)
- LZO1X decoder ported (`src/lzo.rs`); streams parsed (`src/jade_stream.rs`).
- All 6,639 LZO entries decode, zero failures; size.grs terminators
  agree 6,639/6,639. 758.2 MB decompressed from 435.6 MB stored.
- 579 `_wow_` worlds = 96% of content. Full inventory: `bf_ls --stats`.
  Spec + verification: docs/FORMATS.md.
- Remaining M2 thread (folds into M3): deep per-entry classification
  (textures/geometry/animation inside each wow's record stream) comes
  from parsing the wow record format itself — that's M3's first step.

## M3 — Texture + first geometry render ✅ (2026-10-22)
- Wow record walker (`src/wow.rs`), texture parser + DXT decoders
  (`src/texture.rs`), GEO v7/v8 parser (`src/geometry.rs`), PNG writer
  (`src/png.rs`), survey/export tool (`wow_ls`).
- Menu3D verified end to end: 775 records, 128 textures, 76
  geometries; flame-atlas texture decoded to PNG (visually confirmed),
  largest geometry wireframed (coherent architecture) + OBJ export.
- `t2t_game` now builds a de-indexed Bevy mesh (positions/normals/UVs)
  from the world's largest geometry and spawns it with camera/light;
  release build clean, all test suites green. Window opens on a
  desktop (this VM is headless).
- Carried into M4: texture-stub → pixel-data binding (materials!),
  MRM/skin block parsing, GAO object transforms (records are currently
  rendered in local space, unassembled).

## M4 — World assembly: GAO transforms + materials ✅ (2026-10-22)
- GAO parser (`src/gao.rs`), material parser (`src/material.rs`
  incl. the leaf-kind texture-key reader), assembler
  (`src/assemble.rs`), textured software renderer (`wow_render`).
- Menu3D fully assembled in world space: 353 GAOs → 102 geometries,
  84,887 tris; 798/798 elements bound through GAO → material →
  texture to pixel data in the same wow. Previews verified visually:
  the main-menu scene (skyline/cloud skybox ring around the Tower of
  Babylon island city). `t2t_game` spawns the merged world.
- (Rescoped 2026-10-22: assembly had to precede character work —
  nothing renders in place without GAO transforms.)

## M5 — Character: skin + skeleton ✅ (2026-10-22; animation clips open)
- Skin block parsed (`src/geometry.rs`): bone lists, bind matrices,
  (vertex, weight) pairs. Verified on 100_Quai's three characters;
  Kaileena (1,443 pts, 49 bones) renders in bind pose, visually
  confirmed — headpiece, gown, cape, tattoos all correct.
- Skeleton recovered as data: characters' bones are GAOs named
  "B_<char> ..." linked by father keys + local matrices (Kaileena:
  50 bone GAOs, 49 skinned). Static "FAKE_" stand-ins identified.
- OPEN: animation clip banks. Jade-Toolkit has no animation parser
  to port. Hunt log 2026-10-22 (docs/FORMATS.md): clips located
  (gro 0x2002b family per character), records are bit-packed with a
  repeating 28 B block header + 12 B quantized f32 key triples;
  small records tile exactly into blocks.
- UPDATE (later 2026-10-22): the small-record family is DECODED —
  fixed 72-byte units per bone (`src/anim.rs`, tests green): the
  A-block holds a compressed rotation (packing still open), the
  B-block the bone translation as f32 triples, with mirrored L/R
  pairs verified on Kaileena's 35-unit record.
- UPDATE (2026-10-22, third session): the A-block rotation packing
  is CRACKED — smallest-three quaternion (top-2-bit largest index,
  3×10-bit fields, [-1/√2, 1/√2]); validated on 179 harvested units
  with zero invariant violations and exact bind-rotation matches;
  `anim::decode_rotation` + tests shipped. Remaining: track→bone
  mapping (NOT GAO record order — skin-block order is the prime
  candidate), B-block frame semantics, replace-vs-delta convention,
  and the 0x2002b long-clip grammar.
- UPDATE (2026-10-22, fourth session): the LBS SKINNING PIPELINE is
  fully built and verified (`src/bin/pose_test.rs`,
  `tests/skin_real.rs`): pelvis-GAO-global frame bridge x composed
  locals x GEO inverse-binds x sum-normalized weights reproduces
  Kaileena's bind pose exactly (render:
  `out/kaileena_skinned_bind.png`). The 72 B-unit records are
  generic track lists (unit counts 3–39, not bone counts); three
  mapping sweeps failed and the track→object binding must live in
  a referencing TRL record.
- UPDATE (2026-10-23): the binding records are FOUND — 24 small
  descriptor records per level cluster reference (animation data
  key + header-record key); the 142 B header records hold a
  42-entry [i][v][0xfd] table read as the track→bone remap
  (1-based bone numbers, 0xff = unmapped). Remaining: parse all
  descriptors/headers, verify remaps against the three Quai rigs,
  then the 0x2002b clip key-run grammar.
- UPDATE (2026-10-23, 01:00): FIRST PLAYBACK RENDER achieved —
  clip 0x420035a0's rotation stream (253 keys, ~14.2 s) applied
  to Kaileena's L UpperArm gives a clean natural pose (arm
  raised) through the verified skinning pipeline. The clip
  walker is in anim.rs with a green real-data test. Remaining
  for full animation: segment the clip's other track streams,
  resolve the directory-row → channel → header-entry chain,
  decode the translation side, render full multi-track poses.

## M6 — Gameplay core
- Movement/climbing/traversal on level collision, camera, combat
  (dual-weapon system, speed kill), enemies + AI states, Dark Prince
  transformation, health/sand systems, checkpoints/save.
- UPDATE (2026-10-23): foundation slice DONE — collision soup +
  ground queries (src/collision.rs), character controller with
  gravity/jump (src/controller.rs), real actor spawn (FAKE_
  stand-in location, floor-snapped), WASD + orbit follow camera
  in t2t_game; all verified on real data (tests/m6_real.rs).
  Remaining: wall collision, traversal/climbing, locomotion
  clips + the Prince as player character, combat, enemies/AI,
  health/sand, checkpoints.

## M7 — Audio, video, menus
- [x] Menu shell (2026-10-24): boot into the real Menu3D scene as
  backdrop + orbit camera + title UI; Enter transitions into
  gameplay (Phase resource, run_if gates; staging T2T_START=play /
  T2T_MENU_SECS). Framing measured with menu_probe.
- [x] Sound banks → Bevy audio (2026-10-24): DARE cracked end-to-end
  (src/dare.rs, census-clean over all 162 banks); menu music (stream
  event 0x700000) + Menu_Boot UI sounds wired in t2t_game.
- [x] Sword-impact foley (2026-10-24): bank TableA is the sound-event
  catalog; event 0x170024 (weapons' blockSound, proven via
  fn_PlayBlockImpact) resolves through container TableB 26 to waves
  27/28/29; any strike_connects hit plays a random variant in game.
  Verified headless: 47 impact log lines, all 3 variants, 0 panics
  (out/evidence/foley_fight6.log). Container format corrected the
  same day: child count @+0x1c, 16-byte {child, weight} entries —
  the +0x24 read shipped one silent build that played nothing.
  Other combat foley (swing/hurt/death) still needs proven
  semantics; the other Amir AI events in bank 003 are unwired.
- [x] Bink videos replaced by decoded playback (2026-10-24): boot
  intro sequence (UBISOFT/UBIMC/LOGORP/GO_GEN) via ffmpeg-extracted
  frame cache (out/video_cache), streamed into a fullscreen image
  with per-clip audio; Cine/ story clips not yet triggered in-game.
- [x] World texture binding (2026-10-24): the merged world mesh is
  split by resolved texture key (Quai: 71 groups, 70 decoded;
  Menu3D: 53 groups, all decoded) with real DXT/PAL8 textures
  sampled via StandardMaterial (Repeat wrapping; cutout textures
  use alpha masking). Verified headless against the old flat-color
  build: dock props now show plank/striping detail, 0 panics,
  foley still firing. PAL8 decodes as greyscale (no palette yet);
  characters/enemies still flat-tinted — their material chain is
  not resolved by the character loader.
- [x] Character/enemy texture binding (2026-10-24): each skin's
  element textures resolve through its owning GAO's grm chain
  (M_PrinceBare, m_Kaileena, M_Sand_Guard, ...), stack into one
  atlas (Prince: 3 DXT5 keys -> 512x1088; enemies: 1 key each ->
  256x256), and the corners' UVs remap into it — the one-mesh
  animation path is untouched. Verified headless: the flat beige
  fighters now show skin, cloth and trim detail, 0 panics,
  foley still firing. PAL8 skins render greyscale (palette not
  found yet — see NOTES); the Prince is full colour.
- [ ] Menus/HUD per the original MENU data (beyond the shell).

## M8 — Campaign parity
- All sectors wired in original order, scripted events/triggers,
  boss fights, difficulty tuning vs the original, full playthrough
  test start-to-finish.

## Honest constraints
- This is the largest kind of project in the folder — OpenJade took
  its author years. Expect M2–M3 in days-scale iterations, M4+ in
  weeks-scale, with a versioned build at each milestone.
- Behavior is matched against the original running side-by-side; where
  the original exe's logic must be read, REA decompilation gets set up
  (Ghidra provider) rather than guessed.
