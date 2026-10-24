//! Ubisoft DARE audio: .fat index, .sb0 sound banks,
//! and the primer-IMA section codec.
//!
//! Reverse-engineered from the game files and
//! cross-checked against PopAudioStudio v1.3's
//! decoder logic (see docs/FORMATS.md and
//! out/dare/pas_decoder_spec.md).

// ---------- .fat index ----------

/// One file packed in a DARE .big archive.
#[derive(Debug, Clone)]
pub struct FatEntry {
    pub big_offset: u32,
    pub size: u32,
    pub name: String,
}

/// Parse a .fat index: a sequence of records
/// `[u32 self_offset][u32 big_offset][u32 size]
/// [u32 zero][u32 name_len][name bytes incl NUL]`;
/// record size = 20 + name_len; entries tile the
/// .big exactly.
pub fn parse_fat(data: &[u8]) -> Vec<FatEntry> {
    let rd = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 20 <= data.len() {
        let name_len = rd(pos + 16) as usize;
        if name_len == 0 || pos + 20 + name_len > data.len() {
            break;
        }
        let raw = &data[pos + 20..pos + 20 + name_len];
        let name = String::from_utf8_lossy(raw)
            .trim_end_matches('\0')
            .to_string();
        out.push(FatEntry { big_offset: rd(pos + 4), size: rd(pos + 8), name });
        pos += 20 + name_len;
    }
    out
}

// ---------- .sb0 bank (kind 3, The Two Thrones) ----------

/// A decoded-able wave record from a bank's TableB.
#[derive(Debug, Clone)]
pub struct BankSound {
    /// Index within TableB.
    pub index: usize,
    /// The record's own id (TableB +0x00); for
    /// stream-catalog banks this is the event id
    /// (e.g. 0x700000 = the menu music track).
    pub id: u32,
    /// Declared frame count (source metadata; the
    /// decoder formula is the true output length and
    /// may exceed this by 0..=2 for IMA sounds).
    pub frames: u32,
    pub sec1_len: u32,
    pub sec2_len: u32,
    /// Sample rate in Hz (a literal per-sound value).
    pub rate: u32,
    pub channels: u32,
    /// true = primer-IMA sections, false = raw PCM.
    pub ima: bool,
    /// Payload offset within the sound's chunk.
    pub offset_in_chunk: u32,
    /// Chunk id holding the payload.
    pub chunk: u32,
    /// Absolute bank offset of the payload (unused
    /// for stream links).
    pub data_offset: usize,
    /// Region-C name, when the record carries one.
    pub name: Option<String>,
    /// Stream link: when the name is a stream file
    /// (e.g. "STREAM.SS0"), the payload lives in
    /// that file at this offset instead of in a
    /// bank chunk.
    pub stream_offset: Option<u32>,
}

pub struct Bank {
    pub sounds: Vec<BankSound>,
    /// TableA event map: (event id, TableB index).
    /// E.g. bank 002's events 0x190002..0x19000b are
    /// the Menu_Boot / menu-AI sounds.
    pub events: Vec<(u32, usize)>,
    /// Multi-sound containers (TableB type 4): an
    /// event whose record lists child wave indices
    /// (variants of one sound — the engine picks one
    /// per play). Maps TableB index -> child TableB
    /// indices. E.g. bank 003 event 0x170024 (the
    /// weapons' blockSound) -> waves 27, 28, 29.
    pub containers: Vec<(usize, Vec<usize>)>,
}

impl Bank {
    /// Resolve a Jade sound event id to its wave
    /// records: directly when the TableB record is a
    /// wave, or through its container's children.
    pub fn resolve_event(&self, event: u32) -> Vec<&BankSound> {
        let Some((_, tb)) = self.events.iter().find(|(id, _)| *id == event) else {
            return Vec::new();
        };
        if let Some(s) = self.sounds.iter().find(|s| s.index == *tb) {
            return vec![s];
        }
        if let Some((_, children)) = self.containers.iter().find(|(i, _)| i == tb) {
            return children
                .iter()
                .filter_map(|c| self.sounds.iter().find(|s| s.index == *c))
                .collect();
        }
        Vec::new()
    }
}

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// Parse a kind-3 (T2T) .sb0 bank: 28-byte header
/// {magic 0x00150000, countA, countB, countC,
/// regionC size, -1, -1}, then TableA (104 B/rec),
/// TableB (120 B/rec), region C (string blob),
/// chunk index pairs {id, size}, then the chunk
/// payloads concatenated in pair order.
pub fn parse_bank(bank: &[u8]) -> Result<Bank, String> {
    if bank.len() < 28 || u32le(bank, 0) != 0x0015_0000 {
        return Err("not a kind-3 (T2T) .sb0 bank".into());
    }
    let (count_a, count_b, count_c, size_c) = (
        u32le(bank, 4) as usize,
        u32le(bank, 8) as usize,
        u32le(bank, 12) as usize,
        u32le(bank, 16) as usize,
    );
    let table_b = 0x1c + count_a * 104;
    let region_c = table_b + count_b * 120;
    let mut p = region_c + size_c;
    let mut chunks: Vec<(u32, usize, u32)> = Vec::new(); // (id, payload offset, size)
    let mut cur = p + count_c * 8;
    for i in 0..count_c {
        let id = u32le(bank, p + i * 8);
        let size = u32le(bank, p + i * 8 + 4);
        chunks.push((id, cur, size));
        cur += size as usize;
    }
    p += count_c * 8;
    let _ = p;
    let chunk_off = |id: u32| chunks.iter().find(|c| c.0 == id).map(|c| c.1);
    let mut sounds = Vec::new();
    for i in 0..count_b {
        let rec = table_b + i * 120;
        if u32le(bank, rec + 4) != 1 {
            continue; // only type-1 records are waves
        }
        let s = rec + 0x40; // sub-block
        let fmt_field = u32le(bank, rec + 0x34);
        let has_sec2 = u32le(bank, rec + 0x30) != 0;
        let tail_a = u32le(bank, rec + 112);
        let tail_b = u32le(bank, rec + 116);
        let chunk = if fmt_field != 0 {
            1
        } else if tail_a == tail_b || tail_b == 1 {
            2
        } else {
            0
        };
        let offset_in_chunk = u32le(bank, rec + 0x10);
        // A bank may carry no chunk payloads at all
        // (bank 000 is a pure streaming catalog);
        // stream links resolve without a chunk, and
        // anything else unresolvable gets a sentinel
        // offset that decode_sound rejects.
        let base = chunk_off(chunk).unwrap_or(usize::MAX);
        let name = if u32le(bank, rec + 0x2c) != 0 {
            let noff = region_c + u32le(bank, rec + 0x64) as usize;
            let end = bank[noff..]
                .iter()
                .position(|&b| b == 0)
                .map(|z| noff + z)
                .unwrap_or(bank.len());
            Some(String::from_utf8_lossy(&bank[noff..end]).into_owned())
        } else {
            None
        };
        let stream_offset = name.as_ref().and_then(|n| {
            let up = n.to_ascii_uppercase();
            if up.ends_with(".SS0") || up.ends_with(".LS0") {
                Some(offset_in_chunk)
            } else {
                None
            }
        });
        sounds.push(BankSound {
            index: i,
            id: u32le(bank, rec),
            frames: u32le(bank, s),
            sec1_len: u32le(bank, s + 4),
            sec2_len: if has_sec2 { u32le(bank, s + 0x0c) } else { 0 },
            rate: u32le(bank, s + 0x14),
            channels: u32le(bank, s + 0x1c),
            ima: fmt_field != 0,
            offset_in_chunk,
            chunk,
            data_offset: base + offset_in_chunk as usize,
            name,
            stream_offset,
        });
    }
    let mut events = Vec::new();
    for i in 0..count_a {
        let rec = 0x1c + i * 104;
        events.push((u32le(bank, rec), u32le(bank, rec + 8) as usize));
    }
    // Type-4 TableB records are containers: child
    // count at +0x1c, region-C offset at +0x0c of
    // 16-byte entries {u32 child TableB index, u32
    // weight (sums to 65536 across siblings), 0, 0}.
    // (An earlier read put the count at +0x24; that
    // field is always 0 and produced empty child
    // lists — verified against all 27 containers in
    // bank 003, e.g. tb26 -> 27/28/29.)
    let mut containers = Vec::new();
    for i in 0..count_b {
        let rec = table_b + i * 120;
        if u32le(bank, rec + 4) == 4 {
            let n = u32le(bank, rec + 0x1c) as usize;
            let coff = region_c + u32le(bank, rec + 0x0c) as usize;
            let mut children = Vec::new();
            for k in 0..n {
                if coff + k * 16 + 4 <= bank.len() {
                    children.push(u32le(bank, coff + k * 16) as usize);
                }
            }
            containers.push((i, children));
        }
    }
    Ok(Bank { sounds, events, containers })
}

// ---------- section codec ----------

const STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442,
    11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const INDEX_DELTA: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

#[derive(Clone, Copy)]
struct ImaState {
    pred: i32,
    idx: i32,
}

impl ImaState {
    fn step(&mut self, nib: u8) -> i16 {
        let stepv = STEP[self.idx as usize];
        let diff = (stepv * ((nib as i32 & 7) * 2 + 1)) >> 3;
        self.pred = if nib & 8 != 0 { self.pred - diff } else { self.pred + diff };
        self.pred = self.pred.clamp(-32768, 32767);
        self.idx = (self.idx + INDEX_DELTA[nib as usize]).clamp(0, 88);
        self.pred as i16
    }
}

/// Decode one section of a sound into interleaved
/// i16 PCM. `ima` selects the primer-IMA format;
/// otherwise the section is raw PCM copied verbatim.
pub fn decode_section(sec: &[u8], ima: bool, channels: u32) -> Result<Vec<i16>, String> {
    let ch = channels as usize;
    if !ima {
        if sec.len() % (2 * ch) != 0 {
            return Err("PCM section not frame-aligned".into());
        }
        return Ok(sec
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect());
    }
    let hdr = 28 + 20 * ch;
    if sec.len() < hdr || sec[0] != 5 || sec[0x0c] != u8::from(channels == 2) {
        return Err("invalid DARE IMA section header".into());
    }
    if sec[0x12] >= 89 || (channels == 2 && (sec[0x16] >= 89 || sec[0x18] == 1)) {
        return Err("invalid DARE IMA section header".into());
    }
    let trailer = sec[0x18] == 1;
    let mut out: Vec<i16> = sec[28..hdr]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    let i16le = |o: usize| i16::from_le_bytes([sec[o], sec[o + 1]]);
    let mut states = [
        ImaState { pred: i16le(0x10) as i32, idx: sec[0x12] as i32 },
        ImaState { pred: i16le(0x14) as i32, idx: sec[0x16] as i32 },
    ];
    for &b in &sec[hdr..] {
        let (hi, lo) = (b >> 4, b & 0x0f);
        if ch == 2 {
            let l = states[0].step(hi);
            out.push(l);
            let r = states[1].step(lo);
            out.push(r);
        } else {
            let a = states[0].step(hi);
            out.push(a);
            let c = states[0].step(lo);
            out.push(c);
        }
    }
    if trailer {
        out.push(i16le(0x1a));
    }
    Ok(out)
}

/// Decode a whole sound (1–2 sections) from its bank.
/// Some records carry everything in the section-2
/// slot with sec1_len == 0 — decode whichever
/// sections are present, in order.
pub fn decode_sound(bank: &[u8], s: &BankSound) -> Result<Vec<i16>, String> {
    if s.data_offset >= bank.len() {
        return Err("sound payload not in bank (definition-only record)".into());
    }
    decode_sound_from(&bank[s.data_offset..], s)
}

/// Decode a sound whose payload starts at `d[0]`
/// (used for stream links, whose payload lives
/// outside the bank).
pub fn decode_sound_from(d: &[u8], s: &BankSound) -> Result<Vec<i16>, String> {
    let mut pcm = Vec::new();
    let mut off = 0usize;
    for len in [s.sec1_len, s.sec2_len] {
        if len > 0 {
            let sec = &d[off..off + len as usize];
            pcm.extend(decode_section(sec, s.ima, s.channels)?);
            off += len as usize;
        }
    }
    Ok(pcm)
}

/// Encode 16-bit PCM as an in-memory WAV file.
pub fn wav_bytes(pcm: &[i16], channels: u32, rate: u32) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let mut w: Vec<u8> = Vec::with_capacity(44 + data_len as usize);
    w.extend(b"RIFF");
    w.extend((36 + data_len).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes()); // PCM
    w.extend((channels as u16).to_le_bytes());
    w.extend(rate.to_le_bytes());
    w.extend((rate * channels * 2).to_le_bytes());
    w.extend((channels as u16 * 2).to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend(data_len.to_le_bytes());
    for v in pcm {
        w.extend(v.to_le_bytes());
    }
    w
}

/// Minimal 16-bit PCM WAV writer.
pub fn write_wav(path: &str, pcm: &[i16], channels: u32, rate: u32) -> std::io::Result<()> {
    std::fs::write(path, wav_bytes(pcm, channels, rate))
}
