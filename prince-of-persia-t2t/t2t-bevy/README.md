# t2t — Prince of Persia: The Two Thrones, reimplemented in Rust + Bevy

A clean-room reimplementation of the Jade-engine game *Prince of Persia:
The Two Thrones* (2005), built the same way OpenJade and ScummVM-style
projects work: **our own engine code in Rust on Bevy, reading the data
files from the user's own copy of the game**. No original assets are
shipped or redistributed; you point the program at your own `pop3.bf`.

Reference build (verified 2026-10-22): GOG 2.0.0.5, `pop3.exe` v1.0.0.188,
SHA-256 `5f48434c…4aa`, `pop3.bf` 497 MB / 6,648 entries.

## Status — Milestone 5 (2026-10-22): characters ✅ (clips open)
- MRM/skin blocks parse fully: bone lists, bind matrices, weights.
  100_Quai's three skinned characters verified; Kaileena renders in
  bind pose (out/kaileena.png) — 1,443 points, 49 bones, every
  vertex weight-covered. Skeletons are GAO hierarchies (50 bone
  GAOs for Kaileena, father-linked) — recovered as data.
- The Prince himself loads per-level like every character; his
  skinned mesh parses through the same path (survey pending in a
  Prince-bearing sector).
- Open: animation clip banks (no toolkit oracle exists — real RE).

## Milestone 4 (2026-10-22): world assembly ✅
- GAO parser (names, global matrices, visual gro/grm keys, hierarchy),
  material parser (single/multi/multitexture + leaf-kind texture-key
  reader), world assembler, and `wow_render`, a textured software
  renderer for previews on this headless VM.
- Menu3D assembled in world space: 353 GAOs → 102 geometries,
  84,887 triangles, 798/798 elements bound to textures. The renders
  in `out/menu3d_render*.png` show the actual main-menu scene — a
  panoramic skyline/cloud skybox ring around the Tower of Babylon
  island city. `t2t_game` spawns the merged, vertex-coloured world.
- Next: M5 — MRM/skin parsing + skeleton/animation, the Prince.

## Milestone 3 (2026-10-22): first world content ✅
- Wow record walker, texture parser/decoders (BGRA/PAL8/DXT1/DXT3/
  DXT5/4-bit grey), GEO v7/v8 geometry parser, minimal PNG writer,
  and the `wow_ls` survey/export tool.
- Menu3D (the 3D menu world) verified: 775 records, 128 textures,
  76 geometries. A DXT5 flame atlas decoded to a viewable PNG; the
  largest geometry (4,114 pts / 2,835 tris) wireframes as coherent
  architecture and exports to OBJ. `t2t_game` builds that geometry
  into a Bevy mesh with camera + light (release build clean).
- Samples in `out/`. Tests: `tests/wow_real.rs` asserts the census.
- Next: M4 — GAO transforms + texture binding (assemble a whole area
  textured, in place), MRM/skin parsing, then the Prince.

## Milestone 2 (2026-10-22): decompression + inventory ✅
- `src/lzo.rs` + `src/jade_stream.rs` — LZO1X decoder and Jade stream
  layer. All 6,639 LZO entries in pop3.bf decode with **zero failures**;
  size.grs terminator offsets agree **6,639/6,639**. 758.2 MB of game
  data now readable (579 world files = 96%). `bf_ls --stats` prints the
  full inventory; format spec in `docs/FORMATS.md`.
- Next: M3 — parse a wow world's record stream, extract its textures
  and static geometry, render the first area in Bevy.

## Milestone 1 (2026-10-22): archive layer ✅

- `src/bigfile.rs` — full Jade BIG (`.bf`) reader: header, FAT
  descriptors, file/ext/dir tables, old/new ext-size auto-detection,
  path reconstruction, entry extraction. Format cross-checked between
  Jade-Toolkit's `BigFile.cpp` and the real `pop3.bf` byte-by-byte.
- `bf_ls` — CLI: inventory + extract any entry by index.
- `t2t_game` — Bevy 0.16.1 app entry: boots, indexes the archive at
  Startup (build with `--features bevy-app`). Full build verified on
  this Linux VM (8m25s, zero warnings); it boots through Bevy's plugin
  init and stops only at window creation — this VM is headless (no
  DISPLAY/Wayland), so the window itself first opens on a desktop
  (the target is the user's Windows PC anyway).
- Integration test parses the **real** `pop3.bf` and asserts the
  verified facts (v38, universe key `0xcc00008e`, 6,648 files, 5 dirs,
  first entry `ps2old.ico` 59,388 B). `cargo test` — green.

## Run

```bash
cargo test                                   # archive tests (no system deps)
cargo run --bin bf_ls -- path/to/pop3.bf     # inventory the archive
cargo run --features bevy-app --bin t2t_game -- path/to/pop3.bf
```

## Layout

- `src/bigfile.rs` — BIG archive reader (milestone 1)
- `src/bin/bf_ls.rs` — archive CLI
- `src/bin/t2t_game.rs` — Bevy app (milestone 1: boot + index)
- `docs/ROADMAP.md` — the path from here to a 1:1 game
- `docs/FORMATS.md` — reversed format notes, as they land

## Tooling used

- Jade-Toolkit (kaminoer) — format reference + future asset cross-checks
- REA (rea.tools) — binary inspection of `pop3.exe` (identity verified;
  native decompilation provider not installed on this VM yet)
- OpenJade (dekamaru) — proof an independent Jade reimplementation is
  feasible, and the behavioral reference
