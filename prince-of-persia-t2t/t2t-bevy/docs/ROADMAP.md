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
  Hurt foley wired 2026-10-24 (bank 003 event 0x17000f;
  strikes play hurt, block variants reserved for a future
  parry state). Swing/death still have no clean lead and
  stay unwired; ground/wall (0x170010) is identified but
  has no faithful trigger; the other Amir AI events in
  bank 003 are unwired.
- [x] Bink videos replaced by decoded playback (2026-10-24): boot
  intro sequence (UBISOFT/UBIMC/LOGORP/GO_GEN) via ffmpeg-extracted
  frame cache (out/video_cache), streamed into a fullscreen image
  with per-clip audio; Cine story clips (P3C_010..070, 640x448,
  ~10 min total, 18,075 frames) play through a Phase::Cine player
  that suspends gameplay and resumes it on end (verified headless:
  mid-gameplay interruption, resume to gameplay, clip-over-menu
  returning to menu, 0 panics). Staged via T2T_CINE/T2T_CINE_AT;
  real story-beat triggers await M8 campaign work.
- [x] World texture binding (2026-10-24): the merged world mesh is
  split by resolved texture key (Quai: 71 groups, 70 decoded;
  Menu3D: 53 groups, all decoded) with real DXT/PAL8 textures
  sampled via StandardMaterial (Repeat wrapping; cutout textures
  use alpha masking). Verified headless against the old flat-color
  build: dock props now show plank/striping detail, 0 panics,
  foley still firing. PAL8 decoded with its sector palette from
  2026-10-24 (prefix key at offset 52; Quai cutout groups 20→25
  once PAL8 alpha went real); characters/enemies still
  flat-tinted at this point — their material chain was not
  resolved by the character loader yet.
- [x] Character/enemy texture binding (2026-10-24): each skin's
  element textures resolve through its owning GAO's grm chain
  (M_PrinceBare, m_Kaileena, M_Sand_Guard, ...), stack into one
  atlas (Prince: 3 DXT5 keys -> 512x1088; enemies: 1 key each ->
  256x256), and the corners' UVs remap into it — the one-mesh
  animation path is untouched. Verified headless: the flat beige
  fighters now show skin, cloth and trim detail, 0 panics,
  foley still firing. PAL8 palettes cracked 2026-10-24 (prefix
  key at offset 52 → same-sector 1024-byte record; Kaileena,
  Sand Garde, Human Guard, Babylonian all resolve) — enemies
  now render in full colour too, verified headless.
- [x] Menus/HUD per the original MENU data (2026-10-24, verified
  headless: main menu, options tree, pause overlay, bar HUD,
  0 panics, lib tests 9/9): menu text comes from
  POPData.bf's pop4pclocal.dat (INI sections, marker-located —
  the container's name table is only partially reverse-engineered;
  the .MGB/MAGMA layout format is NOT cracked, only its texture
  and font references were read). Real navigable main menu
  (Start New Game / Load Saved Game / Options / Credits / Change
  Profile / Quit), full Options tree (Graphics / Sounds /
  Controls / Game) with the original labels, pause menu on Esc
  (Continue / Options / Quit to Main Menu, with session reset on
  the next Start), quit-confirm dialog, and a segmented bar HUD
  (health red, sand gold) replacing the HP/sand text. Only two
  options toggles work so far (Music on/off, Interface on/off);
  the other options rows are labels with no effect yet. Credits
  video wired 2026-10-24 (CREDITS.BIK via the Cine player,
  music stops/restarts around it). Game Over screen and the
  Special Load dev map menu are NOT wired (checkpoint respawn
  still handles death; MapNames
  from the .dat is kept for M8). Staging: T2T_MENU_SCREEN=<name>
  boots the menu on a screen, T2T_START_PAUSED=1 boots into
  gameplay with the pause overlay open. Implementation note:
  Bevy function systems cap at 16 params — the HUD lives in its
  own hud_system, not animate_char.

## M8 — Campaign parity
- All sectors wired in original order, scripted events/triggers,
  boss fights, difficulty tuning vs the original, full playthrough
  test start-to-finish.
- STEP 1 DONE (2026-10-24, out/sector_census.log + out/campaign_sl.txt):
  campaign order = the 66 MapNames SL entries from POPData.bf's
  pop4pclocal.dat (SL_000 010_Deck ... SL_065 710_MaskSanctuaryV03,
  the last marked "-alternate ending"). Full wow census: 579 wow
  entries, 578 assembled (1 decompress failure:
  500_ExitTo420_GFX_wow), 83 with placed >= 50 (main-sector
  candidates incl. 100_Quai 158/56686, 200_ThroneRoom 118/57805,
  630_Middle_Garden 159/45490, 810_TOWER_P03 252/61571),
  149 with placed == 0 (actor/sound/manager sectors), 346 small
  (GFX/prop/link sectors). Sectors are organized by AREA CODE
  (100 Quai/Gate, 110 Entree, 200 ThroneRoom, 400 Start/Bridge,
  410 RoofTops, 600 MarketPlace, 620/630 Gardens, 710 Fort,
  800/810/820 Tower climbs, 900 SPRoom/SOT/WW).
- STEP 1 FINDING (mapping gap): SL names do NOT appear in wow
  entry names (all 66 substring searches empty), and the exe
  contains no SL strings; TimeLine.bin is a placeholder (size +
  0xffffffff + zeros, no sector data). SL steps therefore do NOT
  map 1:1 to sectors by name — e.g. five ThroneRoom SL visits
  (V01-V05) share one 200_ThroneRoom sector, so sectors are
  REUSED across SL steps with different states/spawns. The
  SL->sector map must come from sector exit/portal trigger
  data or the save system's sector table — that is the next
  M8 research step, not guessable from names.
- STEP 2 DONE (2026-10-24, out/portal_graph.txt): the
  sector-transition graph from in-sector GAOs (78 main
  sectors scanned): ~100 PORTAL edges (Quai<->Depot,
  Depot->InnerCourt, GateEntrance->SiegeTower,
  110 Entree/Ruelle chain, 200 Corridor/Room/ThroneRoom
  chain, Akduk Start->1st->2nd->3rd->4th->ExitTo640,
  620 Courtyard->H01->InnerGarden, H02->SandGate->JctTo630,
  700 Workshop->710, 740 Tomb<->800/TopWell, 900 SOT
  02->03->04->WW_01, WW_02->WW_03, tower 800/810/820
  chain, 400->410->420->500->510->520->600->610->620->630
  ->640->700) plus name edges for link/exit sectors and
  TEC_Sector identity codes (Se_<area>_<num>; a wow file
  holds MULTIPLE sub-sectors, e.g. Plaza has 510_020/040/
  050, RoofPart six). ~30 TEC_Checkpoint GAOs = the
  respawn skeleton. 27 main sectors hold no portal/sector/
  checkpoint GAOs in their own stream (transitions live in
  the NEIGHBOUR's stream or GFX sectors; e.g.
  Middle_Garden's sector object sits in Akduk_Start's
  stream). NO Deck/Beach/Ship wow sectors exist — the
  opening SL steps are cinematic, Quai is the first
  playable sector. No saves exist to mine (profiles are
  header-only), no level/chapter file exists in the BF
  (full name list out/all_entry_names.txt), and the exe
  carries no wow sector names — so the SL->sector binding
  is NOT a static table anywhere found yet. Remaining
  leads: portal GAO record payloads (beyond name/matrix)
  may carry target keys or SL indices; sector behaviour
  scripts may reference SL transitions.
- STEP 3 DONE (2026-10-24, tool wow_rec): portal payload
  archaeology — CLOSED NEGATIVE. Portal GAOs (Quai
  PORTAL_Depot 0x3c03ae3e, Depot PORTAL_Quai 0x3c03ae42)
  are a uniform 198-byte template: name + 68-byte matrix
  + trigger volume (two float triples) + flags + a
  trailing prev-key (own key - 1; the prev-key record is
  itself a 4-byte record holding key - 1, a generic
  record-chain pattern). NO target sector key, NO SL
  index — the target resolves by NAME fragment only.
  TEC_Sector and CheckPoint GAOs share the template (no
  SL data), but checkpoint matrices ARE respawn positions
  (usable for M8 later). Name overlap between SL and wow
  layers is near-zero: only ThroneRoom (5 entries) and
  Garden (16) match; AccessRoom/GuardPost/Sacrifice/
  MechTower/Prison/Library/Forge/MaskSanctuary/Catacombs/
  Deck/Beach have ZERO wow entries. The SL layer is a
  separate localization naming; the SL->sector binding is
  NOT recoverable from static data found so far. What
  M8 can do WITHOUT it: the portal graph gives the first
  verified playable chain Quai->Depot->InnerCourt, so the
  next M8 step is making Depot loadable/playable with
  portal travel — campaign progress that needs no SL map.

## Honest constraints
- This is the largest kind of project in the folder — OpenJade took
  its author years. Expect M2–M3 in days-scale iterations, M4+ in
  weeks-scale, with a versioned build at each milestone.
- Behavior is matched against the original running side-by-side; where
  the original exe's logic must be read, REA decompilation gets set up
  (Ghidra provider) rather than guessed.
