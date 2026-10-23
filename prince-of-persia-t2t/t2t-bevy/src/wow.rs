//! Wow record-stream walker — port of Jade-Toolkit's WowStream /
//! SubEntry logic (from jade_explorer's wow_stream.py / sub_entry.py).
//!
//! A decompressed `_wow_`/`_wol_` stream is a flat sequence of records:
//!   [size:u32][cookie:u32 = 0xEEFFC099][key:u32][payload: size bytes]
//! The payload's first 4 bytes are a type word: an ASCII ".ext" tag
//! (e.g. ".gao") or, when not ASCII, a numeric gro_type (1 = geometry).
//! The remaining size-4 bytes are the object data. The walker locates
//! the first record by scanning the leading window for the cookie with
//! a sane size, then steps 12+size, stopping on a bad cookie/overrun.

pub const WOW_MAGIC: u32 = 0xEEFF_C099;
pub const WOW_MAGIC_ALT: u32 = 0xFEFF_C099;

#[derive(Debug, Clone)]
pub struct WowRecord {
    pub offset: usize,
    pub key: u32,
    pub size: u32,
    /// ".xxx" when the type word is an ASCII extension, else "".
    pub tag: String,
    /// Numeric type when the tag word isn't ASCII (gro_type).
    pub gro_type: Option<u32>,
    /// Offset of the payload within the stream.
    pub body_off: usize,
}

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}
fn is_alpha(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

/// First match of `\.[a-zA-Z]{2,4}\b` in the window (toolkit regex).
pub fn wow_tag_of(head: &[u8]) -> String {
    let mut i = 0;
    while i < head.len() {
        if head[i] == b'.' {
            let mut r = 0;
            while i + 1 + r < head.len() && is_alpha(head[i + 1 + r]) && r < 5 {
                r += 1;
            }
            if (2..=4).contains(&r) {
                let after = i + 1 + r;
                if after >= head.len() || !is_word(head[after]) {
                    return String::from_utf8_lossy(&head[i..i + 1 + r]).into_owned();
                }
            }
        }
        i += 1;
    }
    String::new()
}

/// Offset of the first record's size field, scanning up to max_scan.
pub fn find_first_record(d: &[u8], max_scan: usize) -> Option<usize> {
    if d.len() < 12 {
        return None;
    }
    let limit = max_scan.min(d.len() - 12);
    let end_incl = (limit + 4).min(d.len() - 4);
    let magic = WOW_MAGIC.to_le_bytes();
    let mut off = 0usize;
    while off <= end_incl {
        // find next cookie occurrence at or after off
        let mut i = off;
        let mut found = None;
        while i <= end_incl {
            if d[i..i + 4] == magic {
                found = Some(i);
                break;
            }
            i += 1;
        }
        let i = found?;
        if i >= 4 {
            let rec_off = i - 4;
            let size = le32(d, rec_off) as usize;
            if rec_off + 12 + size <= d.len() {
                return Some(rec_off);
            }
        }
        off = i + 4;
    }
    None
}

pub fn walk_wow(d: &[u8]) -> Vec<WowRecord> {
    let mut out = Vec::new();
    let Some(first) = find_first_record(d, 0x400) else {
        return out;
    };
    let mut off = first;
    while off + 12 <= d.len() {
        let size = le32(d, off);
        let magic = le32(d, off + 4);
        if magic != WOW_MAGIC && magic != WOW_MAGIC_ALT {
            break;
        }
        let key = le32(d, off + 8);
        if off + 12 + size as usize > d.len() {
            break;
        }
        let body = off + 12;
        let head_len = (size as usize).min(80);
        let head = &d[body..body + head_len];
        let tag = wow_tag_of(head);
        // Type word interpretation: ASCII ".ext" => tag; else gro_type.
        let gro_type = if size >= 4 {
            let w = le32(d, body);
            // ASCII extension word: starts with '.' followed by letters
            let is_ext = d[body] == b'.' && d[body + 1].is_ascii_alphabetic();
            if is_ext {
                None
            } else {
                Some(w)
            }
        } else {
            None
        };
        out.push(WowRecord {
            offset: off,
            key,
            size,
            tag,
            gro_type,
            body_off: body,
        });
        off += 12 + size as usize;
    }
    out
}

impl WowRecord {
    /// The object data: payload after the 4-byte type word.
    pub fn data_range(&self) -> std::ops::Range<usize> {
        if self.size >= 4 {
            self.body_off + 4..self.body_off + self.size as usize
        } else {
            self.body_off..self.body_off
        }
    }
}
