# PopAudioStudio v1.3 — DARE audio decoder spec (reverse-engineered)

Source binary: `PopAudioStudio.exe` (kaminoer), a C++ (MSVC) x86-64 application.
All facts below were derived by static disassembly (objdump) of that binary and
cross-checked against the real T2T game banks in
`Sound/sound.big` / `Sound/soundlocal.big`. Function addresses are VMAs
(image base `0x140000000`). Anything not fully proven is marked **UNRESOLVED**.

Key landmarks: `0x14007c348` = CRT malloc, `0x14007c38c` = CRT free.
IMA step table (89 × u32, standard values 7..32767) at file offset `0xb51b0`;
IMA index-delta table (i32: −1,−1,−1,−1,2,4,6,8, repeated per nibble) at file
offset `0xb5170`. Both are the standard IMA-ADPCM tables.

---

## 1. The two payload codecs

A decoded "sound" is 16-bit PCM, mono or stereo, at a per-sound sample rate.
Each sound's payload consists of **one or two sections** (§5). Each section is
independently one of:

| Section layout id | Meaning |
|---|---|
| `0` | "DARE ADPCM" section = 28-byte header + raw-PCM **primer** + IMA-ADPCM payload (§2, §3) |
| `4` | Raw PCM section: the section bytes are copied to the output verbatim (§4) |

Any other layout id is rejected: the decoder throws
`"Unverified decoder layout is disabled."` (VMA `0x1400b6548`, xref
`0x140078dc7` in fn `0x140078990`); the encoder throws
`"Unverified encoder layout is disabled."` (VMA `0x1400b65c8`).

There is **no second custom codec**: the thing the UI calls "DARE ADPCM" *is*
the primer-IMA section of §2. The 28-byte header validator (fn `0x140079780`)
and the frame counter (fn `0x140052fc0`) both operate on exactly that format.

---

## 2. IMA section format (layout 0) — byte layout

Header size helper fn `0x14004b590`:
`header_size(channels) = 28 + 20 * clamp(channels, 1, 2)`.

All multi-byte fields are little-endian. Offsets are from the section start.

| Offset | Type | Meaning |
|---|---|---|
| `+0x00` | u8 | Tag, must be **5** |
| `+0x01..+0x0b` | — | Unused by the decoder (encoder zeroes them) |
| `+0x0c` | u8 | Stereo flag: **1 iff channels == 2**, else 0. Decoder requires equality with `(channels == 2)` |
| `+0x0d` | u8 | Unused |
| `+0x0e` | u16 | Primer frame count, always **10** in files the encoder writes. (The decoder hard-codes 10 frames of primer; this field is informational — see UNRESOLVED note A) |
| `+0x10` | i16 | Predictor seed, channel L ( = last primer sample of ch 0, encoder-written) |
| `+0x12` | u8 | Step-index seed, channel L (encoder writes 0; decoder requires < 89) |
| `+0x13` | u8 | Unused/padding |
| `+0x14` | i16 | Predictor seed, channel R |
| `+0x16` | u8 | Step-index seed, channel R (decoder requires < 89 for stereo) |
| `+0x17` | u8 | Unused/padding |
| `+0x18` | u8 | Trailer flag: 1 = one extra raw sample follows the ADPCM stream (mono odd tail). Must NOT be 1 for stereo |
| `+0x19` | u8 | Unused/padding |
| `+0x1a` | i16 | Trailer sample value (only meaningful when `+0x18 == 1`) |
| `+0x1c` | i16 × (10 × channels) | **Primer**: the first 10 frames as raw interleaved PCM (20·channels bytes) |
| `+0x1c + 20·ch` | u8[] | **ADPCM payload**: 1 byte per 2 mono samples, or 1 byte per stereo frame (see §3) |

Verified against real data: bank `002.sb0`, first sound section at bank offset
1164 begins `05 07 91 7c 30 27 72 43 02 00 00 00 00 eb 0a 00 …` —
byte 0 = 5, `+0x0c` = 0 (mono), u16 @ `+0x0e` = 10, seed = 0, idx = 0,
`+0x18` = 1 (trailer present).

### Frame-count formula (fn `0x140052fc0`, the sample counter)

```
frames(section) = 10 + trailer_flag + 2 * (section_len - (28 + 20*ch)) / ch
```
(integer division; `trailer_flag` = 1 iff byte @ `+0x18` == 1).
Total output samples (i16 count) = `frames * ch`; the decoder caps the total
at `0x20000000` samples, else throws `"Decoded audio is too large."`
(VMA `0x1400b65a8`).

### Validation (decoder fn `0x140078990`, header validator fn `0x140079780`)

A section is rejected (throws `"Invalid DARE IMA ADPCM header or truncated primer."`,
VMA `0x1400b6570`, xref `0x140078d64`) unless ALL hold:

- `section_len >= 28 + 20*channels`
- byte @ `+0x00` == 5
- byte @ `+0x0c` == (channels == 2 ? 1 : 0)
- byte @ `+0x12` < 89, and for stereo byte @ `+0x16` < 89
- for stereo: byte @ `+0x18` != 1

The standalone counter fn `0x140052fc0(params, data, len)` dispatches on
`params+0x2c` (codec): `0` → raw PCM (requires `len % (2*channels) == 0`,
else `"Incomplete PCM frame."`, VMA `0x1400b26b0`); nonzero → DARE section
(requires `len >= 28 + 20*clamp(ch,1,2)`, else `"Truncated ADPCM section."`,
VMA `0x1400b26c8`, xref `0x14005312a`; then fn `0x140079780` must pass, else
`"Invalid DARE ADPCM header."`, VMA `0x1400b26e8`, xref `0x14005314d`).
Note: fn `0x140053620`, called by the counter with size 28, is a generic C++
buffer copy helper, **not** a header parser.

---

## 3. IMA decode algorithm (fn `0x140078990`, layout-0 path)

Per-channel state: `pred` (i32), `idx` (i32, 0..88), seeded from the header
(`+0x10/+0x12` for L, `+0x14/+0x16` for R) — **not** from the primer's last
sample at decode time (the encoder stores them redundantly; in real files
seed == last primer sample, idx seed == 0).

```
output  = primer bytes copied verbatim (10 frames, interleaved)
state_L = (pred = i16@+0x10, idx = u8@+0x12)
state_R = (pred = i16@+0x14, idx = u8@+0x16)   // stereo only
for each payload byte b:
    hi = b >> 4; lo = b & 0xF
    if stereo: emit step(state_L, hi); emit step(state_R, lo)   // interleaved L,R
    else:      emit step(state, hi);   emit step(state, lo)     // sequential
if trailer_flag: append i16 @ +0x1a as the final sample (mono only)

step(state, nib):                                  // core ~0x140078c30;
    stepv = STEP_TABLE[state.idx]                  // second nibble via helper
    diff  = (stepv * ((nib & 7) * 2 + 1)) >> 3     // fn 0x140078e20, state
    if nib & 8: state.pred -= diff                 // passed as {i32 pred; i32 idx}
    else:       state.pred += diff
    state.pred = clamp(state.pred, -32768, 32767)
    state.idx  = clamp(state.idx + INDEX_DELTA[nib], 0, 88)
    return i16(state.pred)
```

**HIGH nibble is decoded first.** This is standard IMA-ADPCM
(aka DVI/Intel) with the standard tables. The only DARE-specific parts are
the 28-byte header, the verbatim 10-frame primer, the header-carried seeds,
and the optional raw trailer sample for odd-length mono sounds.

### Encoder (for reference; fn `0x140078ef0`, PCM → section)

- Requires `pcm_len % (2*channels) == 0` and at least 10 frames, else throws
  `"DARE ADPCM needs at least 10 frames per section."` (VMA `0x1400b65f0`).
- Writes tag 5, stereo flag, u16 @ `+0x0e` = 10; copies the first 10 frames
  as the primer; sets seeds = last primer sample per channel, idx seeds = 0.
- Encodes the remaining frames with a greedy per-nibble search
  (helper fn `0x140079300`) that simulates `step()` and picks the nibble
  minimising error; packs high nibble first. If a mono frame count is odd,
  sets the trailer flag and stores the last sample raw at `+0x1a` instead of
  encoding it.

---

## 4. Raw PCM section (layout 4)

Fn `0x140078990` layout-4 branch: requires `section_len % (2*channels) == 0`,
then appends the section bytes to the output unchanged (via helper
`0x1400788e0`, "append raw bytes"). No header, no transform. 16-bit LE PCM,
interleaved if stereo (stereo PCM does not occur in the T2T banks; see §7).

---

## 5. Sound assembly (fn `0x14004fda0`, "decode_sound")

```
decode_sound(sound):                       // sound = SoundInfo, §6
    len1 = sound.sec1_len                  // +0x54
    len2 = sound.has_sec2 ? sound.sec2_len /* +0x5c */ : 0
    if len1 + len2 != sound.data_len: throw "Sample section sizes do not match the payload."
                                            // VMA 0x1400b2828
    pcm  = decode_section(sound.data + 0,    len1, layout = sound.layout /* +0x194 */, channels)
    if len2: pcm += decode_section(sound.data + len1, len2, layout, channels)
```

Call sites: `0x14005064c`, `0x14005327f`. The counter fn `0x140052fc0` is
called per section from the prepare routine (~`0x1400543fe`/`0x14005441a`),
and each section's frame count is stored (after a checked u32 cast,
fn `0x140058890`) at SoundInfo `+0x60` (section 1) and `+0x64` (section 2).

---

## 6. .sb0 bank file format (parser fn `0x140043260`)

### 6.1 Bank kinds

The first u32 selects the bank layout version ("kind"):

| First u32 | Kind | Plausible game | TableA rec | TableB rec | Sub-block offset in TableB rec |
|---|---|---|---|---|---|
| `0x000a0004` | 1 | Warrior Within | 100 | 128 | `0x30` |
| `0x00120009` | 2 | The Sands of Time | 108 | 132 | `0x30` |
| `0x00150000` | 3 | **The Two Thrones** | 104 | 120 | `0x40` |

Any other first u32 is rejected (error string near VMA `0x1400b24e0`,
`"Truncated or unreadable archive."` family). All offsets/sizes below are for
kind 3 (T2T) unless noted; kind 1/2 differences are listed where known.

### 6.2 Layout

```
+0x00  u32  magic/version (see table above)
+0x04  u32  countA          (# TableA records)
+0x08  u32  countB          (# TableB records = sound slots)
+0x0c  u32  countC          (# chunk index pairs)
+0x10  u32  regionC_size    (byte size of region C, the string blob)
+0x14  u32  (bank 002: 0xFFFFFFFF) — UNRESOLVED (see note B)
+0x18  u32  (bank 002: 0xFFFFFFFF) — UNRESOLVED (see note B)
+0x1c  ...  regions in order:
```

| Region | Offset | Size |
|---|---|---|
| TableA | `0x1c` | `countA * recA` |
| TableB | `0x1c + countA*recA` | `countB * recB` |
| RegionC (NUL-terminated string blob) | after TableB | `regionC_size` bytes |
| Chunk index pairs | after region C | `countC * 8` bytes: `{u32 chunk_id, u32 chunk_size}` |
| Chunk payloads | after the pairs | concatenated in pair order; pair *i*'s payload starts at `payloads_start + sum(size[0..i])` |

The parser bounds-checks every region against the file length and registers
each pair into a chunk table of 24-byte records
`{u32 id; u32 size; u64 pair_file_offset; u64 payload_file_offset}`
(deduplicated by id). The payload cursor for each pair is stored at record
`+0x10`.

**Cross-check, bank 002** (`countA=5, countB=5, countC=2, regionC_size=0`):
TableA @ 28..548, TableB @ 548..1148, pairs @ 1148..1164 =
`{(id=1, size=26746), (id=0, size=2216)}`, payloads @ 1164..30126 =
exactly the file size (30,126). ✔
**Cross-check, bank 000** (`countA=239, countB=859, countC=0,
regionC_size=21708`): computed end of tables = 149,672 vs file size 149,704 —
32 trailing bytes are not referenced by any table (tolerated by the parser).
**UNRESOLVED** (note C).

### 6.3 TableB record (120 bytes, kind 3) → SoundInfo

Only records with **u32 @ `+0x04` == 1** ("wave" records) get the full parse;
other type values produce stub SoundInfos (parser also collects indices of
records whose type == 5 for kinds 2/3 — type `0x0c` for kind 1 — into a
separate list; semantics **UNRESOLVED**, note D). Mapping (record →
SoundInfo field, parser fn `0x140043260`, mapping block at `0x1400436xx`):

| TableB offset | Type | SoundInfo field | Meaning |
|---|---|---|---|
| `+0x00` | u32 | `+0x04` | Record id (e.g. `0x0019000b`) |
| `+0x04` | u32 | — | Record type; **1 = wave** |
| `+0x08` | u32 | `+0x08` | (copied; bank 002: equals section-1 length — role **UNRESOLVED**, note E) |
| `+0x10` | u32 | `+0x10` | **Offset of this sound's data within its chunk** (cumulative; see §6.5) |
| `+0x2c` | u32 | bool `+0x28` | "Has name in region C" flag (kind 3). Kinds 1/2: offset `+0x24` |
| `+0x30` | u32 | bool `+0x70` | **Has section 2**. Kinds 1/2: offset `+0x28` |
| `+0x34` | u32 | — | **Chunk/format field**: nonzero ⇒ DARE-IMA sound; zero ⇒ raw-PCM sound. Kinds 1/2: offset `+0x2c` |
| `+0x40` | — | — | Sub-block `S` begins (kind 3). Kinds 1/2: `+0x30` |
| rec end `−8` | u32 | — | Tail A (used by the chunk-selector rule, §6.5) |
| rec end `−4` | u32 | — | Tail B (used by the chunk-selector rule, §6.5) |

Sub-block `S` (at record `+0x40` kind 3 / `+0x30` kinds 1/2):

| S offset | Type | SoundInfo field | Meaning |
|---|---|---|---|
| `S+0x00` | u32 | `+0x50`, copied to `+0x14` | **Declared frame count** of the original sound (exact for PCM; for IMA it is the source frame count — decoder output can differ by 0..2 frames, §7) |
| `S+0x04` | u32 | `+0x54` | **Section 1 byte length** |
| `S+0x08` | u32 | `+0x58` | (copied; 0 in bank 002 — **UNRESOLVED**) |
| `S+0x0c` | u32 | `+0x5c` | **Section 2 byte length** |
| `S+0x10` | u32 | `+0x68` | A second rate-like value in Hz (**UNRESOLVED**, note F) |
| `S+0x14` | u32 | `+0x18` | **Sample rate (Hz)** — stored verbatim, no code/table |
| `S+0x18` | u32 | `+0x20` | Bits per sample; must be **16** |
| `S+0x1c` | u32 | `+0x1c` | **Channels** (1 or 2) |
| `S+0x20` | u32 | `+0x24` | Format tag: must be **3** for IMA sounds, **0** for PCM sounds |

(Kind 1 deviates: `+0x20`/`+0x1c` are read as u16s from record `+0x48`/`+0x4a`,
and `+0x24` from record `+0x4c`.)

Derived SoundInfo fields computed by the parser:

- `+0x00` = record index within TableB.
- `+0x0c` = total payload length = `sec1_len + (has_sec2 ? sec2_len : 0)`
  (0 ⇒ the sound is flagged with error text `"Empty audio payload."`,
  VMA `0x1400b23f8`).
- `+0x2c` = codec flag for the counter: `1` if chunk/format field ≠ 0
  (DARE section), else `0` (raw PCM).
- `+0x194` = section layout for the decoder: `0` (IMA) if the chunk/format
  field ≠ 0, else `4` (raw PCM).
- `+0x6c` = **chunk selector**: the id of the chunk (from the index pairs)
  that holds this sound's data. Computed (block at `0x1400439df`):
  `1` if the chunk/format field (`+0x34`) ≠ 0; otherwise, from the record's
  last two u32s (tail A @ recB−8, tail B @ recB−4): `2` if tail A == tail B
  or tail B == 1, else `0`. (Partially interpreted — note G.)
- `+0x98` = per-sound error string (0x2d = 45 bytes max) when validation
  fails; empty when the sound is valid.
- `+0xb8`/`+0xc0` = resolved data pointer/length, filled in later by the
  prepare routine (§6.5), not by the parser.
- Validation (block at `0x140043a4a`): sample rate ∈ [1, 384256],
  bits == 16, and format tag == 3 (IMA) / 0 (PCM) consistent with the
  chunk/format field; any failure stores
  `"Unsupported sample codec or audio parameters."` (VMA `0x1400b23a0`)
  as the sound's error text.

**Names**: for kind 3, if the `+0x28` flag (record `+0x2c`) is set, the
sound's name is the NUL-terminated string at
`regionC + u32le(record + 0x64)` (bounds-checked against `regionC_size`),
stored at SoundInfo `+0x78`. For kinds 1/2 the name is embedded in the
record itself at record `+0x54` (kind 2) / `+0x50` (kind 1, capped at
recB−8 bytes), stored at SoundInfo `+0x78` if the flag is set, else at
`+0x30`. (Which of `+0x30`/`+0x78` is "name" vs "path" is **UNRESOLVED**,
note H.)

### 6.4 TableA record (104 bytes, kind 3)

Role only partially decoded. Verified fields from bank 002 (all 5 records):

| Offset | Type | Observed | Meaning |
|---|---|---|---|
| `+0x00` | u32 | `0x00190002`, `…07`, `…08`, `…09`, `…0b` | Id (same id space as TableB record ids) |
| `+0x04` | u32 | 1 | Type (?) |
| `+0x08` | u32 | 0,1,2,3,4 | **Index of the TableB record** this entry refers to (matches TableB order in bank 002) |
| `+0x10` | u32 | `0x00010000` | **UNRESOLVED** |
| `+0x18` | u32 | `0xFFFFFFFE` | **UNRESOLVED** |

TableA is not needed to locate or decode wave payloads; the parser's payload
path uses TableB + the chunk table only.

### 6.5 Locating a sound's payload (prepare routine, ~`0x1400543xx`)

```
chunk   = chunk_table.find(id == sound.chunk_selector /* +0x6c */)
data    = bank_bytes + chunk.payload_file_offset + sound.offset_in_chunk /* +0x10 */
length  = sound.sec1_len + (sound.has_sec2 ? sound.sec2_len : 0)
```
Then §5 decodes `data[0..len1]` and `data[len1..len1+len2]` as sections.
(The prepare routine stores the resolved pointer at SoundInfo `+0xc0`, a
companion pointer at `+0xb8`, and per-section frame counts at `+0x60`/`+0x64`
via fn `0x140052fc0`.)

**Cross-check, bank 002**: chunk id 1 payload @ 1164, size 26746 =
6872 + 7259 + 6350 + 6265 = the four IMA sounds' section-1 lengths, at
in-chunk offsets 0, 6872 (`0x1ad8`), 14203 (`0x3733`), 20553 (`0x5001`) —
matching TableB `+0x10` exactly. Chunk id 0 payload @ 27910, size 2216 =
the single PCM sound's length (`0x8a8`). ✔

---

## 7. Sample rate

There is **no rate-code table**: the sample rate is a plain u32 Hz value
stored per sound in sub-block field `S+0x14` (§6.3). Empirical census over
all 162 T2T banks in `sound.big` + `soundlocal.big` (2,342 wave records,
all parsed with §6 and passing the tool's own validation rules):

- Codec/channel combos are exactly three: (mono, IMA, fmt 3) × 749,
  (stereo, IMA, fmt 3) × 741, (mono, PCM, fmt 0) × 852. No stereo PCM,
  no other combos.
- Rates seen at `S+0x14`: 24000 (most), 16000, 22050, 20000, 18000, 12000,
  11025, 14000, 17000, 8000, 44100 (×1), plus near-miss values (18002,
  18003, 18004, 17998, 20050) — i.e. rates are computed/pitch-adjusted per
  sound, confirming they are literal Hz, not codes.
- Declared frame count (`S+0x00`) vs decoder-formula frame count: exact for
  PCM (820/820 chunk-verified records). For IMA: exact in 56 verified
  records, −1 in 1, −2 in 37 (**UNRESOLVED** why the encoder's declared
  count sometimes sits 1–2 below the decoded count — treat `S+0x00` as
  source metadata, and the §2 formula as the true output length).

## 8. Function map (for re-checking any claim)

| Address | Role |
|---|---|
| `0x140043260` | .sb0 bank parser (§6); called from the archive-open path at `0x140051c18` |
| `0x14004b590` | `section_header_size(channels) = 28 + 20*clamp(ch,1,2)` |
| `0x14004fda0` | decode_sound: two-section assembly (§5) |
| `0x140052fc0` | frame counter / codec validator (§2) |
| `0x140053620` | generic buffer-copy helper (NOT format logic) |
| `0x1400788e0` | append raw bytes to output vector (PCM path) |
| `0x140078990` | section decoder dispatcher: layout 0 = IMA, layout 4 = raw PCM (§2–§4) |
| `0x140078e20` | single-nibble IMA step on a `{i32 pred; i32 idx}` state |
| `0x140078ef0` | section **encoder** (PCM → section) (§3) |
| `0x140079300` | encoder greedy nibble search |
| `0x140079780` | IMA section header validator (§2) |
| `0x140058890` | checked u64→u32 cast (used for per-section frame counts) |

Bank-object fields set by the parser (object returned by `0x140043260`):
`+0x00/+0x08` chunk-table vector (24-byte records), `+0x10/+0x18` input
buffer ptr/length, `+0x54` countA, `+0x58` countB, `+0x5c` regionC size,
`+0x60` recA size, `+0x64` recB size, `+0x68` payloads start offset,
`+0x70` vector of SoundInfo pointers (0x1d0 bytes each).

## 9. UNRESOLVED items

- **A.** The decoder hard-codes a 10-frame primer and takes the payload
  offset as `28 + 20*channels`; whether the u16 @ section `+0x0e` is ever
  honoured (values other than 10) is untested — no non-10 value was
  observed in T2T banks.
- **B.** Bank header u32s at `+0x14`/`+0x18` (both `0xFFFFFFFF` in bank 002)
  are not read by the parser code identified; possibly loop/default markers
  written by the bank authoring tool.
- **C.** Bank 000 has 32 trailing bytes beyond the last table region that
  no table references; purpose unknown (padding/footer?).
- **D.** TableB record types other than 1 (wave): the parser collects
  type-5 (kind 2/3) / type-0x0c (kind 1) record indices into a separate
  list — plausibly "stream/group" records — but their fields were not
  decoded. They do not affect wave decoding.
- **E.** TableB `+0x08` is copied to SoundInfo `+0x08` but its semantics
  are unclear (in bank 002 it equals the section-1 length; elsewhere it
  may be a size or flags word).
- **F.** Sub-block `S+0x10` is a second Hz-valued field (e.g. 11025 when
  the rate is 22050; 24000 when the rate is 24000; 32000/36000/40000 also
  occur). Likely a base/source rate vs the (possibly pitch-shifted)
  playback rate at `S+0x14`. The tool validates and uses `S+0x14`
  (→ SoundInfo `+0x18`); no decoder code path reads `S+0x10`.
- **G.** The chunk-selector rule (§6.5) is reconstructed from the parser's
  tail-comparison code and fits bank 002 exactly (IMA sounds → chunk 1,
  PCM sound → chunk 0), but the semantic meaning of tail A/tail B (record
  `+112`/`+116` in kind 3) is unknown — they may be chunk id + flags, or
  loop points that coincidentally encode the chunk choice.
- **H.** SoundInfo has two string slots (`+0x30`, `+0x78`); which one the
  UI treats as the display name vs a category/path is undetermined.
- **I.** Kind-1 (WW) TableB field reads at `+0x48..+0x4c` are partly
  byte-wise (u16 assemblies); the kind-1 mapping in §6.3 is approximate.
  Kinds 1/2 record layouts otherwise follow the same scheme with the
  sizes/offsets tabled in §6.1.
