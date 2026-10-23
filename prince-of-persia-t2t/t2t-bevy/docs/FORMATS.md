# Reversed format notes — Jade / Two Thrones (GOG build)

Every claim here is verified against the user's real files, and where a
second source exists (Jade-Toolkit source, size.grs) the agreement is
stated. pop3.bf SHA-256 `00134ea9…b600`.

## BIG archive (pop3.bf) — M1
See `src/bigfile.rs` header. Header v38, FAT at 0x44, FileTableExt entry
84 B (old), 6,648 active files, 5 dirs, universe key 0xcc00008e.

## Entry streams (LZO blocks) — M2
An entry's data is usually a block stream:
```
u32 bin_file_size            == the entry's stored length
repeat:
  u32 dec_size               0 => terminator, stop
  u32 comp_size
  comp_size bytes            LZO1X data, decodes to dec_size bytes
                             (if comp_size >= dec_size: stored raw,
                              dec_size bytes follow instead)
```
- dec_size <= 131,072 (LZO_BLOCK_SIZE = 0x20000).
- Bytes after the terminator are trailing padding; the terminator's
  offset is the entry's "declared length" in size.grs.
- Decoder: `src/lzo.rs` (state-machine port of lzo1x_decompress_safe),
  stream layer: `src/jade_stream.rs`.

### Verification on pop3.bf (2026-10-22)
- 6,639 / 6,648 entries are LZO streams; **all 6,639 decode with zero
  failures**, output sizes matching every block header.
- size.grs cross-check: **6,639 / 6,639 terminator offsets agree** with
  the declared lengths — three independent sources consistent.
- Totals: 435.6 MB stored (entries) -> 758.2 MB decompressed.
- 579 `_wow_` world files = 729.9 MB decompressed (96% of all content);
  largest: `740_Well_TopWell` 9.17 MB, `100_Quai` 8.98 MB, `Menu3D`
  8.95 MB, `800_ClimbUp_P01` 8.80 MB, `730_Garden` 8.27 MB.

### The 9 raw (non-stream) entries
- `ps2old.ico`, `ps2.ico` — PS2 memory-card icons (length-prefixed raw)
- `versiontag.bin` — sentinel test pattern (12345678 / DEADBABE…)
- `TimeLine.bin` — starts FFFFFFFF, raw table
- `PS2_Loading_Blank.bin`, `PS2_Loading_Bar.bin` — PS2 loading images
- `500_ExitTo420_GFX_wow_ff0f5d49.bin` — the one wow outlier; inspect in M3
- `WOLInfo.bin`, `size.grs` — raw tables

## size.grs — M2
Raw table (not a stream): u32 total byte length, then (key u32,
declared_len u32) pairs. 6,655 rows, 6,642 unique keys in pop3.bf.
Parser: `parse_size_grs` in `src/jade_stream.rs`.


## Wow record streams — M3
A decompressed `_wow_` file is a flat record sequence (port:
`src/wow.rs`, from Jade-Toolkit WowStream/SubEntry):
```
record: [size:u32][cookie:u32 = 0xEEFFC099][key:u32][payload: size]
payload: [type word:u32][object data: size-4]
```
Type word is either an ASCII ".ext" tag (".gao", ".wow", ...) or a
numeric gro_type (1 = geometry). The walker finds the first record by
scanning the leading 0x400 bytes for the cookie with a sane size field,
then steps 12+size, re-checking the cookie each record.

## Textures — M3
Header 44 B, pixels at offset 52 (+4 for fmt 1/5, +8 for fmt 11 v>=4).
Detected by: u32[0] = 0xFFFFFFFF, u32[20] = 0xCAD01234,
u32[28] = 0xC0DEC0DE; logical w/h are u16 at 8/10, actual w/h u32 at
40/44, version u32 at 32, format u32 at 36, mips u32 at 48.
Formats: 0 BGRA, 1 PAL8, 5 DXT1, 6 DXT3, 7 DXT5, 11 4-bit grey.
Parser + decoders: `src/texture.rs` (DXT1/5 standard expansion, DXT3
legacy rounding — toolkit parity).
Note: many wow records are 56–60 B header-only texture stubs; their
pixel data lives in separate records (e.g. Menu3D records 771–773
carry full DXT5 data) or father BF entries — binding is M4 work.

## Geometry (GEO v7/v8) — M3
Detection: gro_type == 1 and payload starts with the GEO header
(10 u32: version 7|8, flags1, flags2, nb_pts, abs_count, has_abs,
nb_uvs, nb_elems, mrm_marker, skin_ok3). Then vertices (3 f32 each),
normals (if skin_ok3 != 0), colours (ARGB, if has_abs != 0), UVs
(2 f32), element headers (nTri, matId), triangles at 16 B each
(3 vertex u16 + 3 uv u16 + pad). Parser: `src/geometry.rs`.
MRM/skin geometries (mrm_marker == 0xC0DE2002) are detected but not
yet parsed (skin block port pending). Menu3D has none; character
meshes will need it (M4).

### Verification on Menu3D (2026-10-22)
775 records walked; 128 textures, 76 geometries detected. Record 772
(DXT5 128×128 flame atlas) decoded to PNG and visually confirmed;
record 398 (4,114 pts / 2,835 tris) wireframe preview shows coherent
architecture; OBJ export written. Asserted in tests/wow_real.rs.


## GAO records — M4
Payload (after the ".gao" tag word), port `src/gao.rs`:
- Header: version u32, editor_flags u32, identity u32, name_size u32;
  name (ASCII, NUL-terminated) at offset 16.
- Global matrix at 16 + name_size + 10: 68 B = 16 floats column-major
  (translation at [12..14]) + type u32.
- Bounding volume: 48 B if identity & 0x80000 (OBBox), else 24 B.
- Visual block (identity & 0x4000): gro_key u32, grm_key u32, then a
  block of 42 B (46 + remaining when the u16 at +38 is nonzero and the
  u32 at +42 is sane).
- Hierarchy block (identity & 0x400000), 72 B: father_key u32 + the
  68 B local matrix.
A gro_key may name a geometry GROUP instead of a GEO: members are
found by scanning the group's payload for words that are themselves
geometry keys (toolkit geo_group_members parity).

## Materials — M4
By gro_type (`src/material.rs`): 3 = single (texture key at +24),
5 = multitexture, 4 = multi (n_sub at +4, positional sub-material
keys from +8; GEO element matIds index this array directly).
Sub-material payloads in T2T are "leaf" records whose first u32 is a
kind 4–9; their texture key sits at a kind-dependent byte offset
(kind 4/5/6 → 40, 7 → 42, 8 → 50, 9 → 63) — `resolve_texture_key`,
the reader the toolkit's own exporters use. The plain multitexture
parse alone resolves NOTHING in Menu3D; the leaf reader resolves all.

## Assembly + verification — M4
`src/assemble.rs` + `wow_render` (software rasterizer, Z-up,
perspective-correct UVs, z-buffer, lambert shade).
Menu3D: 353 GAOs (290 visual) → 102 geometries placed, 84,887 tris,
798 elements — ALL 798 resolved GAO → material → texture key → pixel
data carried in the same wow (0 stub-only, 0 missing). Rendered
previews (out/menu3d_render*.png) show the main-menu scene: a
panoramic skyline/cloud skybox ring around the Tower of Babylon
island city — assembly, transforms and binding all visually correct.


## Skin block (MRM geometries) — M5
When the GEO header's mrm_marker == 0xC0DE2002, offset 36 is NOT
skin_ok3: it is the skin block (toolkit parse_skin_data parity):
flags u16, list count u16, then per bone list: bone_idx u16,
num_verts u16, bind matrix (16 floats), matrix_type u32, then
num_verts × (vertex u16, weight u16). A fresh u32 after the block
gates the normals array; vertex data follows it. Weights are u16
fractions of full scale 0xFFFF (verified on Kaileena: every vertex
covered, per-vertex sums ≤ 0x10000).
Parser: `src/geometry.rs` (GeoInfo.skin: flags + Vec<SkinBone>).

## Characters & skeletons — M5 survey (100_Quai)
- Skinned GEOs in Quai: M_Human_Guard (1,017 pts, 42 bones),
  m_Kaileena (1,443 pts, 2,310 tris, 49 bones), m_Babylonian_Soldier
  (837 pts, 33 bones). Character meshes are in METRES (Kaileena is
  1.84 tall, feet at z=0) — levels use a ~100x larger unit; character
  placement comes from the skeleton root at runtime, not the GAO
  matrix stored in the wow.
- The skeleton is the GAO hierarchy itself: 50 "B_Kaileena ..." GAOs
  (Pelvis, Spine, Head, arms with full finger chains, legs, hair and
  cloth MOD bones), each carrying a father key + local matrix; the
  skin drives 49 of them. A static "FAKE_Kaileena" low-poly GAO also
  exists as a distant stand-in.
- Animation clip banks are NOT yet parsed: Jade-Toolkit ships no
  animation parser to port (no Anim format in its headers), so clip
  data is a genuine reverse-engineering target (format probing or
  REA work on pop3.exe), tracked as the deep half of M5.


## Animation (TRL) records — hunt log, 2026-10-22 (PARTIAL)
Classification (Jade-Toolkit AssetIndex, the only oracle): records
with gro_type >> 16 == 0x0002 (and low byte not 0x24 sound / 0x30 AI)
are ANIMATION. The C++ toolkit deliberately never parses them; its
Python predecessor had a `parse_trl` returning a track count for the
display name "Animation (Nt)" — the Python source is not public.

Census in 100_Quai (per character key family, e.g. Kaileena
0x420035xx): gro 0x2002b = the big clips (3 records, 34–56 KB for
Kaileena; 3.4–5.2 KB for the 0x42006dxx family), gro 0x20006 = small
records (220 B – 2.9 KB), plus 0x20003/0x20007/0x20024-0x2004e
variants. Sample records extracted to `out/anim/rec{883,872,941,874,
498}.bin` (record indices within 100_Quai) for continued analysis.

Structure findings (verified on the samples):
- The bodies are BIT-PACKED, not byte structs: naive u32 reads show
  floats recurring at shifted alignments.
- A repeating block grammar exists: block header (28 B) begins
  `03 <b1> <b2> <b3> (0x10|0x20) 00 00 00 02 00 00 00 ...` followed by
  an optional payload. The 216 B record 883 tiles EXACTLY into 6
  blocks of alternating 28/44 B; record 872 shows the same header
  recurring 70×; in the 5,185 B record 941 the first block carries a
  ~1.6 KB payload (long key run) before the next header.
- Payload keys are 12-byte f32 triples, quantized to reduced
  mantissa precision (e.g. exactly 0.04443359375, 0.076751708984375,
  0.12938308715820312), stored at bit-level offsets inside the
  payload (first clean triple sits 4 B into the nominal payload
  region). Identical consecutive triples = static tracks (two copies
  of the same vector appear in record 883's last blocks:
  (0.1599, -0.1581, -0.5925)).
- gro 0x20003 records do NOT match this block pattern (different
  sub-format; header starts `04 b4 ff ff 10 07 00 00 e2 ...`).

Still unknown (the remaining RE): the bit grammar of the 28 B block
header (channel type, key count, time base), the per-channel
quantization widths, key timing/delta encoding, and how tracks map
to skeleton bone indices. Candidate next oracles: REA/disassembly of
the ANI loader in the unpacked pop3.exe (UPX-decompressed copy was
made during the hunt; strings are sparse), or continued differential
probing across same-rig clips of different lengths.


## Animation — first family DECODED (2026-10-22, later session)
The small gro 0x20006/0x20036 records are arrays of fixed 72-byte
units, one per animated bone — implemented in `src/anim.rs`
(`is_pose_record` / `parse_pose_record`), tested in
`tests/anim_real.rs`:
- unit = block A (28 B) + block B (44 B); [0]=0x03, [1]=0xFA (0xFE
  on unit 0), index u16 at [2] (0xFFFF on unit 0, then 0,1,2,…),
  size words 16 (A) / 32 (B) at [4], channel word 2 at [8].
- A carries a 4-byte compressed rotation at [19..23], a zero
  separator, and the same 4 bytes again at [24..28].
- B carries the bone translation as 3 x f32 at [47..59], a zero
  separator, and the same 12 bytes again at [60..72].
Verified on Kaileena's record 0x4200359f: 35 units tile the record
exactly (2,520 B), indices run 0..=33 after the root unit, all
translations finite and humanoid-scale, with the expected mirrored
left/right pairs (e.g. bones 2/6: (-0.088, 0.042, ±0.133)). A second
record (0x420035a5) holds 3 units of the same family.
Still open: the A-block rotation packing (smallest-three quaternion
and fixed-point euler candidates FAILED their norm checks — do not
assume), the track-index → skeleton-bone mapping (35 tracks vs 49
skinned bones), and the gro 0x2002b long-clip segmentation (first
block can carry ~1.6 KB of key data; its grammar differs from the
72 B unit family after the first block).


## Animation — A-block rotation DECODED (2026-10-22, third session)
The A-block's 4-byte value is a **smallest-three compressed
quaternion**: bits [31:30] = index of the largest component; three
10-bit fields follow MSB-first, each mapped [0,1023] ->
[-1/sqrt(2), +1/sqrt(2)]; the largest component is
sqrt(1 - sum of squares), non-negative. (An earlier hand test with
wrong field shifts falsely rejected this — the correct shifts are
20/10/0.) Evidence: 179 units harvested from pose records across
several wows (`wow_ls --dump-pose`): sum-of-squares <= 1 for
179/179, reconstructed-largest invariant violations 0/179, idx
distribution w-dominant (122/179 — typical for real pose data),
58 near-identity rotations, and multiple units matching Kaileena's
bind-pose bone-local rotations to quat distance < 0.002. The root
unit's raw value 0xe0080200 decodes to the exact identity
quaternion. Implemented as `anim::decode_rotation`, asserted in
tests/anim_real.rs.
Still open: track->bone mapping (candidates: GAO record order,
skin-block bone order, core-bone order — a record-order composition
test produced nonsense, so it is NOT plain record order), the
B-block's exact frame semantics (two tracks carry |z| ~= 3.59,
implausible as bone-local translations), and the composition
convention (replace vs delta on bind locals). The decisive
experiment is a full LBS skinning render sweeping mapping x
convention; the skeleton side is ready (bind composition from GAO
locals reproduces her 1.84 m T-pose layout exactly).


## Skinning pipeline VERIFIED (2026-10-22, fourth session)
The full LBS chain now reproduces Kaileena's bind pose exactly
(`tests/skin_real.rs`, render `out/kaileena_skinned_bind.png`,
tool `src/bin/pose_test.rs`):
- Bone order = wow record order of the character's B_* GAOs; the
  GEO skin list's `bone_idx` indexes that order directly.
- skel_global = hierarchy composition of GAO local matrices
  (root/pelvis local = identity).
- geo_global_b = pelvis_gao_global * skel_global_b — the pelvis
  GAO's global matrix is the frame bridge (its rotation is the
  axis swap (x,y,z) -> (z,-y,x), translation = pelvis position).
- skin matrix = geo_global_b * skin bind_matrix (the GEO's stored
  matrices are true inverse-binds in geo frame).
- Bind check: skin matrix == identity (3x4 part; Jade matrices
  carry ~0 in m[15]) for 45/49 bones to <1e-4. Exceptions:
  bone_idx 45..48 (MOD BackCloth R04 + loincloth chain) — their
  bind matrices match the NEXT bone's composed global (shifted by
  one), a cloth indexing quirk not yet explained.
- MRM weights: per-vertex raw sums are 0.25 / 0.5 / 0.75, never
  1.0 — skinning must sum-normalize (bind AABB then reproduces
  the raw geometry AABB to <1e-3).

## Track binding — reframed (2026-10-22)
The 72-byte-unit records are GENERIC track lists: harvested unit
counts per record vary (3, 4, 7, 20, 35, 36, 39) and do not match
character bone counts, so a record's tracks bind to objects via a
referencing structure elsewhere (the TRL/animation-component
graph), not by position alone. Three mapping sweeps against
Kaileena's skeleton (record order / core order / skin order x
replace / rotation-only / delta) all fail; the skin-order render's
apparent coherence is a false positive (bind torso + wrongly
rotated cloth bones). Do not re-run those sweeps. Next: find the
record that binds tracks to GAOs (search wow records referencing
both a pose-record key and bone GAO keys), and/or decode the
0x2002b clip family, whose key families are per-character.


## Animation binding records FOUND (2026-10-23)
Cross-reference scan (bf_ls --find-key over the whole archive;
wow_ls --xref / --list-spans / --save-raw within a wow):
- NO record inside 100_Quai references the animation data keys in
  its payload — the references sit in a block of 24 small
  DESCRIPTOR records (64 B each: 12 B header + 52 B payload)
  clustered at records 744-769 in Quai.
- Descriptor payload layout: [01 ff][ref_key u32 = the animation
  data record][zeros][f32 15.0 at +34][...][tail key u32][01 00
  ...]. The tail key names a 142 B HEADER record (e.g. descriptor
  745 -> header 0x42000313 for tracklist 0x4200359f; descriptor
  746 -> header 0x42000419 for clip 0x420035a0; descriptor 744 ->
  clip 0x420035a7 with tail 0xffffffff + flags instead).
- Header records (126 B data) sit adjacent to their clips in
  record order (873 right after clip 871 and tracklist 872) and
  contain a 42-entry table of 3-byte entries [i][v][0xfd] with i
  running 1..42 and v a small number (1..50) or 0xff — almost
  certainly the track->bone remap table (1-based bone numbers,
  0xff = unmapped). One entry's third byte is 0xfe in header 875,
  so the third byte is data, not a constant marker.
- Samples saved: out/anim/ref{744..747}.bin (descriptors),
  out/anim/hdr{873,875}.bin (headers), out/anim/refblock.bin
  (raw block incl. headers), out/anim/quai_spans.txt (all spans).
Next: parse all 24 descriptors + headers, extract the remap
tables, check them against the guard/soldier/Kaileena bone
counts, then decode a 0x2002b clip's key runs under its header.


## Descriptors + headers parsed (2026-10-23, later)
Descriptor anatomy resolved: the record's TYPE WORD is
0x????FF01 where ???? = the referenced data key's low 16 bits
(ref key = 0x4200???? in Quai's key space); payload (52 B):
[01 ff is part of the type word][ref low half][zeros][f32 at
+34][header key u32 at +42][01 00 00 00][00 00]. The f32 is 15.0
for most animations and 13.0 for three of the second character's
— a per-clip parameter (frames? seconds? rate?).
Quai pairings (Kaileena family): tracklists 0x4200359f (35 units)
and 0x420035a1 (36 units) share header 0x42000313; clips
0x420035a0/0x420035a2 share header 0x42000419; data a3/a4 use
headers 0x42003625/26 (147 B tables, ALL entries unmapped —
placeholder maps); data a5/a6 use 3-byte headers 0x4200359d/9e
(single entry) — the tail field's role varies by family. The
second character's (0x42006dxx) descriptors have NO header
(0xffffffff). Clip 0x420035a7's descriptor also has no header.
Header tables 873/875 (42 entries): only 14 tracks mapped —
v = [3,7,8,50,-,17,18,-,-,4,5,1,-,9,-,10,-,-,-,-,-,-,-,19,-,20,
-,-,-,-,-,-,27(875 adds 28),...]. Read as track->bone (1-based,
bone 50 = last GAO) since bone->track would need track 50 in a
35-track list. Entry count 42 vs 35/36 units in the paired
track lists remains unexplained — the table may cover the clip
family's full channel set.
Clip probe (rec874 = clip 0x420035a0, 34,138 B): opens with an
A-style 28 B block header (03 f4 ff ff 10...02 08 e0...), then at
+32 a second header shape (03 f0 ff ff, size word 0xf0=240,
count word 15 — matching its descriptor's f32 15.0), long zero
runs, then key data. Only ONE unit-style header in the whole
clip: the body is packed key runs, not unit pairs. Segmentation
of the clip body is the next probe.


## Clip two-section structure (2026-10-23, later still)
Verified across five 0x2002b clips (a7 0x420035a7, a0, a2,
6d46, 6d4b): each clip opens with TWO block sections.
- Section 1 at offset 0: [03 xx ff ff][size][count] with
  size = (count/2) x 16 and count in {2,4} — small header block.
- Section 2 follows immediately (offset 28-42, varies): same
  lead shape; size word == track_count x 16 EXACTLY in all five
  (a0: 240=15x16, a2: 336=21x16, a7: 592=37x16, 6d46: 352=22x16,
  6d4b: 32=2x16). So section 2's count word IS the clip's track
  count, and 16 B per track follows the header in some form.
- The 16 B/track rows (rec874) read as mixed float data: one
  word of the form 0x008000xx (xx = 0,1,2,3,11,14,15,20,21,36,
  59,138 — plausible per-track KEY COUNTS) plus f32 values in
  metre range (-2.36..4.06) — first-key data per track is the
  working hypothesis; the packed key stream follows.
- Variant: rec877 (a2) payload starts one byte earlier-looking
  (leading 0x00, leads at +0/+31 in shifted frame) — same
  grammar, byte-0 is a variant/flags byte.
Next: identify the 16 B row layout exactly (which word is the
key count), then walk the packed stream using known key counts
as the checksum; cross-check track counts vs header tables
(a0: 15 tracks, header 875 maps 14 of its 42 slots — the 42
likely counts the rig's full channel set, not this clip's).


## Clip four-section structure (2026-10-23, 00:30)
A full lead scan of rec874 (identities: SEC2 size==count x 16;
SEC1 size==(count/2) x 16) finds FOUR sections, two pairs:
@0 SEC1 (16, 2), @32 SEC2 (240, 15), @288 SEC1 (2032, 254),
@1830 SEC2 (3216, 201). Working model: pair 1 = rotation side
(header + track rows), pair 2 = translation side (or vice
versa); counts 254/201 may be total key counts.
Row alignment: the 0x008000xx count-words sit at 64, 80, 96...
(section2 header = 32 B here), giving 14 clean [count][vec3]
rows before the next lead at 288 — the 15th track is plausibly
the root, described by section 1 (mirrors the unit records,
whose first unit is the root/header unit).
After the second pair's rows, the remaining ~29 KB holds a
packed stream with a repeating 6-byte motif
[delta u16][4-byte value] — e.g. `03 00 32 e5 0a bc` repeated
with deltas 03/06/02 — consistent with (frame-delta, quat)
keys where static channels repeat the same quat.
Next: implement the walker in Rust (anim.rs) — parse sections,
rows, then consume the stream per-track using the row key
counts; the record end is the checksum.


## Key stream anchors — byte-exact (2026-10-23, 00:35)
The packed stream IS 6-byte keys [u16 delta][u32 smallest-three
quat]: a repeated quat value occurs at a perfect 6 B stride,
and phase-locked decoding gives long 100%-valid runs.
rec874 anchors:
- @288 SEC1-style header (A=2032, B=254), 18 B header, then
  EXACTLY 254 keys x 6 B = 1,524 B of rotation keys at
  306..1830 — landing precisely on the next section lead.
  (A = B x 8: 254 keys claim 8 B/key in the header's accounting
  vs 6 B stored — or A counts something else; unresolved.)
- @0 SEC1 (A=16, B=2): 2 keys — plausibly the root track —
  then SEC2 @32 (track rows).
- Clean 6 B runs found: 253, 194, 248, 255, 61, 164, 136,
  178, 163 keys (the 253-run is the @288 stream minus its
  first key, whose delta slot overlaps the header tail).
- @1830 SEC2 (A=3216, B=201) + the remaining 32,308 B are NOT
  yet laid out: 201x16 B of rows + a 6 B-key stream fits the
  byte count (16 + 3216 + 4846x6 = 32,308) but a clean 194-key
  run inside the supposed rows region contradicts pure rows.
  Next: anchor-scan the tail the same way (find where a long
  valid run ends exactly at EOF or at a count boundary).


## Walker ported to Rust + pair 2 identified (2026-10-23, 00:45)
anim.rs now carries find_sections (Directory: size == count*16;
Stream: size == count*8) and read_rot_keys (6 B keys), locked by
tests/clip_real.rs on clip 0x420035a0: sections at 0 (Stream,
2 keys), 32 (Directory, 15), 288 (Stream, 254), 1830; the 254
decoded rotations are all unit quaternions and the stream ends
exactly at 1830. Full suite: 7 real-data test files, all green.
Pair 2 (@1830, count 201) is the TRANSLATION side: its early
floats reproduce tracklist 0x4200359f's root translation
(0.101, -0.1393, ...) exactly. In the tail a second 6 B motif
appears — [packed-vec3 u32][u16 delta of 1-2] with smoothly
varying values — the translation key stream. Still open:
@1830's byte accounting, the packed-vec3 field layout, and how
the 254 rotation keys distribute over the 15 tracks (the rows'
0x008000xx counts sum to 343, not 254 — row semantics still
unresolved). Then: split keys per track, apply the header
remap, render a posed frame.


## FIRST PLAYBACK RENDER (2026-10-23, 01:00)
Clip 0x420035a0's rotation stream is ONE continuous track:
253 keys, deltas 1-3 frames (180 keys at delta 1), total span
425 frames ~ 14.2 s at 30 fps — matching its descriptor's
f32 15.0 = clip duration in seconds. Quats evolve smoothly
(median step 0.0004 in 1-|dot|), angles 100-170 degrees.
Applied via pose_test --render clip<K>b<B> (replace rotation,
keep bind translation) through the verified skinning pipeline:
- on bone_idx 9 (L UpperArm): a clean, anatomically correct
  pose — Kaileena standing naturally, left arm raised high
  (out/anim/c120_b9.png). Head/torso/dress/legs untouched.
- on Spine1 (idx 2): coherent but constant ~90-degree torso
  pitch (wrong frame for that bone); on Head (idx 4): broken.
So the stream drives an upper-arm-class bone; header 875's
table has v=10 (1-based = bone_idx 9) at entry 16. The formal
chain directory-row -> channel -> header entry is the last
unresolved link; the other tail streams (194/248/255/61/164/
136/178/163-key runs) are the clip's other tracks, each in
its own section. Remaining: segment all streams, assign each
via its directory row + header entry, render the full pose.


## FULL CLIP WALK — EOF-EXACT (2026-10-23, 01:15)
Clip 0x420035a0 walks end-to-end with zero unknowns (43
sections, final offset == record length 34,138):
[root rot stream (2 keys)][directory 16 B + 15 rows x 16 B]
then per-channel sections whose lead word byte 2 is the
channel index (0..33): ROT stream = 18 B header + count x
6 B keys; TRANS stream = 18 B header + count x 14 B keys
([u16 delta][f32 x][f32 y][f32 z]). Translation channels:
0, 1, 2, 6, 13, 23, 33 only. Totals: 4,324 rotation keys,
512 translation keys.
Channel 0's first translation key = (0.101, -0.1393, -3.5901)
— exactly tracklist 0x4200359f's root-unit translation, so
channel 0 is the root/placement channel of the clip family.
FULL POSE TEST (header-875 table as channel->bone, frame
200): FAILS — the figure streaks (AABB height 5.86). The
header table does NOT map channels to Kaileena's bones the
way it reads: prime suspect — the 42-entry tables belong to
a 42-bone rig (the Quai GUARD has exactly 42 bones; Kaileena
has 50 GAOs/49 skin bones) or v indexes something else.
Lead for next session: empirical per-channel assignment —
apply each channel's stream alone to each bone and rank by
locality + human-scale AABB; the correct pairing should be
unambiguous. pose_test --render anim<F> holds the attempt.


## FULL POSE — ROTATIONS VERIFIED, TRANSLATIONS ARE SCENE-LEVEL (2026-10-23, 01:35)
Correction to the previous entry: the header-875 channel->bone
table IS right for rotations. The streak failure came from
applying the translation streams as bone-local translations:
those values are scene/placement-frame (channel 0's is the
character root placement), so they poison any bone they touch.
With rotations only (bind translations kept; pose_test
--render anim<F>, translations behind ANIM_APPLY_TRANS):
- Frame 200: a coherent dramatic pose — body turned, one arm
  raised behind the head, cape/loincloth streaming
  (out/anim/anim200_rot.png). AABB human-scale.
- Frame 0: coherent twisted stance (clip may open mid-action).
- Frame 400 (near the 425-frame end): upper body tangles —
  late-frame defect remains (mis-sampled stream ends, or one
  or two channels still misassigned at extreme poses).
So: channel->bone (header) + rotation streams + skinning =
verified animation playback. Remaining: frame-400-class
defects, translation streams' proper role (root placement
for channel 0; others TBD), cloth-bone handling, then the
same treatment for clips a7/a2 + the guard/soldier rigs, and
playback inside the Bevy app (M5 complete).


## Late-frame poses — diagnosis (2026-10-23, 01:45)
Per-channel spans are consistent (351-431 frames; two span
groups ~354 and ~428). The frame-300/400 tangles concentrate
in (a) the cloth/hair slabs — long stiff chains parented to
the spine that sweep huge arcs with no simulation (in-engine
cloth is procedural; our renderer applies rotations raw), and
(b) two genuinely large late swings in mapped channels:
channel 0 -> Spine1 rotates 98.9 degrees between frames
200-400, channel 33 -> L Thigh 130 degrees. The body core
(legs, torso column, feet) stays coherent at every sampled
frame (0/200/300/354/400). Whether the late extremes are the
cinematic's real choreography (a recoil/fall) or two still-
misassigned channels cannot be settled without ground truth
(the game itself or the exe's ANI code) — flagged, not hidden.
Frame 200's pose stands as the verified-good reference.


## ANIMATION SET COMPLETION (2026-10-23, 02:45)
New section layouts found in a2/a7: Rot5 (5 B keys:
[u8 delta][u32 quat]) and Trans13 (13 B keys:
[u8 delta][f32 x 3]); a7 also ends with a cinematic Footer
section (placement vec3s at level scale + event data).
parse_clip classifies all; tests assert EOF-exact walks
with zero Unknown sections for clips a0, a2, a7, 6d46,
6d4b (tests/clip_real.rs, 5 tests green).
Kaileena set COMPLETE:
- a2 renders via header 419 (body coherent; cloth slabs
  point up — the known cloth caveat).
- a7 renders via header 313 (dramatic conjuring pose,
  frame 150). Its descriptor lists no header; 313 (record-
  adjacent, sibling tracklists' map) and 419 give near-
  identical results.
- Tracklist 9f via header 313 (unit index = channel)
  reproduces clip a0's frame-0 pose BYTE-IDENTICALLY
  (same PNG hash) — the two record families are one system:
  tracklists are single-frame snapshots in clip channel
  space. a1 renders as an extreme billowing pose.
NPC (0x42006dxx) set — grammar verified, binding OPEN:
the 6d key block holds the sector's generic NPC placements
(3 soldiers + 3 guards + swords); rigs are B_BabylonSoldier
(35 GAOs; soldier bind render verified standing, 1.92 m)
and B_HG_Bip (guard). Descriptors carry NO header. Two map
hypotheses FALSIFIED for clip 6d46 on the soldier rig:
identity (channel i -> bone i) and Kaileena's map translated
by bone name — both leave him crumpled/lying; channel 0
alone tips him over, so ch0 is not his pelvis under either.
The NPC map likely lives in the shared Technical Bank BF
entry or the small 6d bank records (Quai records 508-516) —
a dedicated dig. pose_test gained T2T_GEO_KEY /
T2T_BONE_PREFIX / ANIM_IDENTITY_MAP / ANIM_NAME_MAP /
ANIM_ONLY_CHAN for this work.


## NPC binding dig (2026-10-23, 03:15)
- The 6d bank is duplicated per level (Quai, FakeCity,
  InnerCourt00 all embed it); descriptors identical, header
  field always 0xffffffff. The small 6d records (508-516)
  are key+1 alias stubs, not maps.
- The soldier's OWN header record exists (key 0x420031eb,
  Quai record 1412, InnerCourt record 140): 35 entries, ALL
  v = 0xff. So the [i][v][x] table is a per-character
  OVERRIDE (populated for Kaileena's cinematic rig, empty
  for NPCs) — NPC binding does NOT come from it.
- Guard bind verified (2.21 m with gear); identity map
  falsified on the guard too (crumpled at frame 100).
- Remaining candidate: the clips' DIRECTORY rows (16 B
  each; a0 has 15 rows for 34 channels, 6d46 has 22 rows
  for ~35 channels) — never decoded; they may carry the
  track->object binding for header-less clips. Next dig:
  decode directory rows across clips and correlate with
  channel/bone counts; or REA on the exe's ANI loader.


## CORRECTION — NPC binding is IDENTITY (2026-10-23, 03:35)
The "identity falsified" verdict above was WRONG, caused by
sampling frames 100-400 on a clip whose dense Rot5 streams
span only ~25-40 frames: the sampler held the final key, so
every render showed the same end pose, and a falling body
read as a broken rig. Within span, clip 6d46 under the
identity map (channel i -> bone i in GAO record order)
renders a coherent FALLING/DEATH animation on BOTH NPC
rigs: the soldier (frames 10/20: body horizontal, limbs
articulated, pose evolving frame to frame) and the HG
guard (frame 10: tumbling, skirt flying). Conclusion: NPC
standard-bip clips bind by record order with no table —
which is why the soldier's header record is all-0xff
(no override needed) and the 6d descriptors carry no
header. Kaileena's cinematic rig is the exception that
needs the override table. Also corrected: 6d46 has NO
directory/path section — its second section is a Trans13
root stream; the path/directory section exists in a0
(15 points, a smooth camera/root path) but is not a
binding structure. The animation set is now complete for
every Quai character: Kaileena (3 clips + 2 tracklists,
override table), soldier + guard (shared 6d bank, identity
binding). Remaining polish: cloth simulation, translation
streams' role in playback, late-frame extremes in a0,
descriptor f32 semantics for the short NPC clips.


## BEVY PLAYBACK WIRED (2026-10-23, 04:10)
The verified pipeline now lives in the library (src/skin.rs:
Character, Clip, ChannelMap, pose_locals, posed_verts) and
t2t_game spawns Kaileena in 100_Quai playing clip 0x420035a0
on a 30 fps loop, re-skinning her mesh every frame (flat
normals recomputed per frame; she stands at the world AABB
center — actor-GAO placement is the M6 refinement).
Parity locked by tests/playback_real.rs: the library
reproduces pose_test's verified AABBs exactly (Kaileena a0
frame 200 via header map; soldier 6d46 frame 10 via
identity). Full suite: 13 tests green. The Bevy app builds
clean (release, bevy-app feature); on this headless VM it
stops at winit event-loop creation (no DISPLAY; under Xvfb
it gets one step further, to libxkbcommon loading, which
the VM lacks) — the window opens on the user's Windows PC,
same as every prior milestone.


## M6 FOUNDATION — COLLISION, SPAWN, CONTROLLER (2026-10-23)
- Collision (src/collision.rs): TriangleSoup from the
  assembled world (Quai: 56,686 tris). ground_height(x, y,
  z_ref, step_up) returns the highest walkable surface
  (normal cos to +Z >= 0.5) at or below z_ref + step_up,
  barycentric-interpolated. Brute force; a grid index is
  future work.
- Actor spawn: a character's B_ rig GAOs assemble at
  rig-local coordinates, NOT level placement. The level
  spot is the sector's FAKE_<char> stand-in GAO: for
  Kaileena in Quai its AABB is [-383.0, 82.0, 57.0]..
  [-367.2, 106.2, 68.3]; ground at its center = 61.95.
  t2t_game spawns her there, feet snapped to the floor.
- Controller (src/controller.rs): feet position + vertical
  velocity; gravity 980 u/s^2, step-up 30, step-down 45,
  jump 420 u/s (~0.9 m). Grounded = snap to ground_height;
  walking off an edge goes airborne. Verified on real data
  (tests/m6_real.rs): a 300-unit drop lands exactly on the
  queried floor (z = 61.954792); walking east off the spawn
  platform falls, as the dock geometry dictates.
- t2t_game controls: WASD move (camera-relative), Shift
  run, Space jump, LMB-drag or arrow keys orbit, follow
  camera. Character yaw = movement direction (model faces
  +Y at rest; facing offset -90 deg, to be eyeball-checked
  on first visual run). Wall collision is M6.2.


## M6.2 — WALL COLLISION + THE PRINCE FOUND (2026-10-23)
- Wall collision (collision.rs resolve_walls): the capsule
  (r=25, h=175) is pushed out of wall triangles (|cos to Z|
  < 0.5, z-range overlapping the capsule above a 20-unit
  step-over) by a face rule (perpendicular foot inside the
  triangle -> push along the wall's XY normal) + an edge
  rule (closest segment point -> radial push), 2 relaxation
  passes. Controller applies it after horizontal motion, so
  movement slides along walls. Verified: synthetic room —
  head-on stops exactly at radius distance, diagonal slides
  the full tangential distance (tests/m6_real.rs); her real
  spawn is not disturbed by the resolver.
- The Prince (archive-wide skin census, src/bin/scan_skins.rs;
  sector GAO dumps, src/bin/dump_gaos.rs):
  * Body (shirtless "final" variant): GEO 0x510022d3 via
    M_PrinceBare.gao — 964 pts, 1,729 tris, 45 skin bones;
    embedded in PrinceFinal_Shape, 999_TransformRoom,
    740_Well_TopWell, 610_Exit.
  * Rig = the B_Pr_ family (83 GAOs in PrinceFinal_Shape):
    B_Pr_Bip body (34: Pelvis..fingers), B_Pr_Face (~20
    facial bones), B_Pr_Hair_*, B_Pr_Sword_L/R, holders,
    B_MOD_skirtUP. Character::load prefix = "B_Pr".
  * Dark Prince / sand form: GEO 0x6d0025b7 via
    M_SandPrince_Body.gao (987 pts, 34 bones, 15 sectors);
    face hires GEO 0x42002fe5 (m_SandPrince_face_hires).
  * Bind check anomaly (OPEN): bones 0-9 exact; bones 10+
    deviate by a near-constant ~0.95 (hair 34-36/43-44 up
    to ~2.05) — likely a hierarchy root difference between
    the cinematic Shape sectors and gameplay sectors, or a
    second frame bridge. The bind render is nevertheless a
    coherent standing T-pose (2.01 m with hair/swords):
    out/prince_bind.png.
  * His gameplay (clothed) body variant and locomotion clip
    bank are NOT yet located — next survey step.
- Headless evidence breakthrough: the Bevy app RUNS on this
  VM under Xvfb + Lavapipe (mesa-vulkan-drivers 25.2.8
  extracted to ~/mesa-vk, VK_ICD_FILENAMES=lvp_icd.json,
  WGPU_BACKEND=vulkan; the GL backend fails on Bevy's depth
  downsample shader). ffmpeg x11grab captures the window —
  real screenshots/video of t2t_game now exist
  (out/evidence/). This retires the old "stops at winit"
  ceiling for future milestones.


## PRINCE PIPELINE VERIFIED (2026-10-23, 11:30)
- prince_diag (src/bin/prince_diag.rs) settles the "bind
  anomaly": through the LIBRARY pipeline the Prince's body
  is sound — every B_Pr_Bip bone's self-deviation is 0.0000
  except Thigh/Calf ~0.19 and UpperArm/Forearm ~0.05 (minor
  residuals, possibly a slight non-bind pose baked into the
  cinematic Shape sector). The hair/sword cluster (skin
  bones 34-44) binds are permuted among themselves (best
  matches are other hair bones at dev 0.00-0.11) — cosmetic
  only, like Kaileena's shifted cloth binds. The earlier
  alarming ~0.95 readings were a pose_test bind-check
  indexing artifact (it compares in skin-list order; the
  Prince's skin list is not in bone order). skin.rs is
  authoritative.
- Clothed body: GEO 0x6d002617 via M_Prince_Body.gao
  (738 pts, 39 bones) exists ONLY in PrinceLoading_Shape,
  where the rig GAOs are stored in a posed (non-bind)
  state — its authoritative bind rig is still to locate
  (possibly a companion bank entry). Also: face highres
  GEOs 0x42004c13/0x42004c92, LArm + LArm_Corrupted
  variants (corruption mechanic) in the Shape sectors.
- Prince animation: PrinceFinal_Shape holds ONE anim
  record, clip 0x42004cd3 (gro 0x2002c, 21 rot channels).
  With the IDENTITY map it renders a coherent airborne
  action pose (out/prince_anim0.png) — the standard-bip
  convention holds for the Prince too.
- t2t_game is now preset-driven: Quai -> Kaileena (header
  map, FAKE_ anchor), PrinceFinal -> the Prince (identity
  map, M_PrinceBare anchor). His full locomotion bank
  (gameplay sectors) is the next survey target.


## ANIMATION BANK MAP + CROSS-SECTOR PLAYBACK (2026-10-23, 12:00)
- scan_anims (src/bin/scan_anims.rs) ranks all 166
  anim-carrying wows. Layout: ACT_Enemy_<name> sectors are
  per-enemy actor banks (SandArcher 101 clips); AMIR_wow is
  the PRINCE's actor sector (Amir = his codename): his rig
  (64 B_Pr GAOs) + a 72-clip bank that recurs byte-identical
  (175,682 clip bytes) across the garden sectors (730_*,
  630_*, 800_ClimbUp...). Char_wow = common acrobatic bank
  (38 clips). Dynamic_Actions_* = specialized sets
  (SpeedKill_prince 27 clips). Area sectors carry long
  cinematics (410_From400: one 324-frame clip).
- prince_clips (src/bin/prince_clips.rs) plays ANY sector's
  clips on ANY sector's character (survey / PRINCE_TRACE /
  PRINCE_RENDER strip modes). Verified visually on the
  Prince (identity map): AMIR bank = TRAVERSAL set —
  0x970001e8 ledge-hang, 0x97000521 ground roll,
  0x33000ee7 dive/glide, 0x33000811 hang idle, 0x970000d3
  ledge pull-up; Char bank 0x1200268b = aerial flip. All
  coherent — cross-sector playback is sound.
- OPEN: his base ground locomotion (stand/walk/run) is in
  none of the surveyed banks — next hunt (descriptor name
  tables in the actor sectors are the likely index).
  Descriptor tail field also reads differently outside
  Quai (0x39aa0000-style values) — layout variant TBD.
- In-game: the Prince preset now state-switches clips —
  grounded = PrinceFinal leap tracklist, airborne = AMIR
  dive clip 0x33000ee7 (cross-sector load in
  prepare_character); spawn lift 250 units so the spawn
  fall reads as the dive.


## DESCRIPTOR FULL-KEY FORMAT + THE PRINCE'S SET (2026-10-23, 12:20)
- Descriptor (52 B payload) decoded byte-exact: type word
  = 0x<low16>FF01; payload+4 (data_range+0) holds the
  referenced key's HIGH half as a u32 (<<16); f32 at
  payload+32 (data_range+28); tail (header key or
  0xffffffff) at payload+40 (data_range+36). Full key =
  (hi << 16) | (type word >> 16). Verified on Quai
  (0x420035a0) and AMIR (0x2d000ad4...).
- The referenced data records are usually gro 0x20025,
  NOT 0x2002b — SAME section grammar, Clip::load plays
  them (another container variant).
- anim_resolve + prince_set (src/bin/) resolve a sector's
  descriptors archive-wide and score the clips on a rig.
  AMIR_wow's 95 descriptors ALL resolve; the set (92
  unique) is duplicated in the garden sectors' own
  descriptor lists. Contents: traversal (hang/shimmy/
  climb/dive/roll), combat reactions, knockdowns/falls —
  visually verified across a dozen rendered strips.
- His BASE locomotion (wait/walk/run) is NOT in the
  descriptor system: it is referenced from his action
  table (the small logic records in AMIR; debug strings
  confirm a movement-action category: "fn_TransitionTo-
  Movement : Action is not wait walk or run"). Decoding
  the action table is the next RE frontier (M6).
- 400_Start's descriptors = the intro cinematic's
  multi-track set (278-frame tracks; 7-10 channel tracks
  are cameras/props).


## ACTION-TABLE XREF — LOCOMOTION VERDICT (2026-10-23, 12:50)
- amir_xref (src/bin/) scans a sector's records for
  aligned u32s matching the 651 true clip-data keys
  (gro 0x20025/0x2002b/0x2002c) archive-wide.
- AMIR's ~380 logic records DO reference clips directly
  (1-2 per record, ~100-250 B records = his action
  definitions) — but they resolve to the known traversal/
  action set plus only 21 extra contextual clips (falls,
  transitions; all scored/rendered — none is a locomotion
  cycle). Gameplay-sector logic (610_Exit) contains NO
  aligned clip-key references at all.
- 500_Entree/110_Entree descriptor lists = more
  cinematics (800-frame and camera-track sets).
- CONCLUSION: the base wait/walk/run clips are bound by a
  mechanism with no plain key references in wow data
  (compiled GAM/action logic, or exe-side tables). The
  decode target for the next frontier is the structure of
  AMIR's action records themselves (they carry parameters
  + the clip refs we found; the movement subset may key
  its anims indirectly via descriptor-record keys).


## ORPHANS + EXTENDED DESCRIPTORS (2026-10-23, 13:45)
- orphan_clips: 300 of 651 clip keys are referenced by
  nothing (no descriptor, no aligned u32 anywhere). Pools:
  enemy banks (their AI binds them by the same hidden
  mechanism), Char-bank CREATURE cycles (the perfectly
  looping 0x12001cxx family at height 1.60 = the chariot
  horse / beast rig — verified visually, quadruped poses
  on a humanoid mesh), Dynamic_Actions extras, and the
  already-known garden traversal extras. NO Prince
  locomotion among them.
- AMIR's "action records" are EXTENDED animation
  descriptors: 118 B / 202 B records embedding a standard
  descriptor block (f32 15.0, tail) with the clip key at
  fixed slot body+52 and parameter words in front of it
  (e.g. 01 00 00 07 / 01 00 10 09 — flags/IDs TBD).
- Loop-closure ranking of his 92: perfect loops are the
  hang idle (0x2d000ad4, RMS 0.017), the wall-shimmy
  (0x33000f52), a wall-lean idle (0x0a00004a) and a
  wall-cling cycle (0x970004e1) — all contextual.
- Four independent methods (descriptor resolution,
  action xref, orphan analysis, loop-closure ranking)
  agree: no ground locomotion cycle exists in the
  key-referenced data. The remaining path is the GAM/
  action-parameter decode or exe-side tables.


## TRAVERSAL GAMEPLAY — M6.3 (2026-10-23, 14:15)
- collision::TriangleSoup::find_ledge(pos, dir, hand_z):
  floor-discontinuity march (10-unit steps to 120, then
  bisection) — a floor in the hand band [hand_z-60,
  hand_z+5] appearing ahead while the character's own
  column has none = a grabbable edge; returns Ledge
  { top (on the floor, inside the edge), normal (unit,
  back into open air) }.
- controller HANG state: grab when airborne, vel_z <= 200,
  ledge in band (HAND_HEIGHT 170; HANG_DROP 178 — feet
  below the ledge top). Shimmy = tangent projection +
  ledge re-snap (same height ±30). Climb: jump while
  hanging -> 0.45 s transition (CLIMB_TIME), lands inside
  the edge, grounded. Drop: MoveInput.drop releases.
- Verified: tests/traversal_real.rs — synthetic platform
  (grab -> hang height exact, shimmy travels, climb lands
  on top, drop falls) + Quai ledge scan finds real ledges.
  Suite 17 green.
- t2t_game: presets gained rig_sector + traversal clip
  set; T2T_CHAR=prince forces the Prince into any world
  sector (rig from PrinceFinal_Shape, world from the CLI
  sector); anchor fallback chain (preset anchor ->
  FAKE_Kaileena -> any FAKE -> world top); T2T_SPAWN_OFF
  (dx,dy,dz) and T2T_SPAWN_YAW staging envs. In-game
  animation: hanging -> hang/shimmy/climb clips
  (0x33000811 / 0x33000f52 / 0x970000d3 from AMIR_wow),
  S drops, Space climbs.
- Capture: Prince staged off the Quai dock's east edge
  (spawn -335, 94, 40, yaw PI) — fell, grabbed the edge
  (top z 61.95), held the hang pose:
  out/evidence/prince_hang_screenshot.png + _video.mp4.
  src/bin/ledge_probe.rs scans staging spots.


## BEAM MECHANICS + BANK SURVEY — M6.4 (2026-10-23, 14:45)
- collision::beam_span(pos, perp): marches both ways
  across the facing-perpendicular; a beam = BOTH edges
  found within 70 units and total width < 100. Returns
  BeamSpan { center, axis, width, z }.
- controller BEAM state: while grounded on a span, the
  lateral position snaps to the centerline every step
  (sideways input absorbed); clears when the support
  widens or the ground-follow walks off the end.
  Verified: tests/traversal_real.rs beam test (land ->
  snap -> travel with lateral push -> end fall) + a real
  Quai anchor (narrow dock plank at (-351, 1), w ~10-20).
- BANK SURVEY (visual + foot-trace): the Dynamic_Actions
  banks hold GRIP traversals, not standing cycles:
  Beam 0x330026c5 = under-beam hand shimmy (feet dangle
  and kick alternately; body horizontal, arms up);
  0x057cca6a / 0x057cca6b = beam side-hangs; 0x330026be =
  prone along the beam; 0x3300580c = low crawl/catch.
  Column 0x4e001797 = pole spin/turn (feet orbit the
  pelvis in the trace); the 0x4e00178x family = pole
  grip/climb poses. Swing bank: bar-hang set (h ~1.4).
- CONSEQUENCE: beam-TOP walking has no dedicated clip in
  these banks — it uses the (still missing) base walk
  cycle, same gap as ground locomotion. Beam clip wiring
  was built, then REMOVED rather than ship a wrong clip;
  the mechanics ship clip-agnostic. In-game beam state
  currently plays the grounded placeholder clip.
