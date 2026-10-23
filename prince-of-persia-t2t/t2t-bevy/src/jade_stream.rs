//! Jade BF entry streams.
//!
//! Most entries are LZO block streams (port of Jade-Toolkit
//! `Compression.cpp`, itself a port of `core/compression.py`):
//!   u32 bin_file_size  (total stream length, including this header)
//!   blocks until terminator: u32 dec_size (0 = end), u32 comp_size,
//!     then comp_size bytes of LZO1X data — or dec_size raw bytes when
//!     comp_size >= dec_size (stored/incompressible block).
//! Block dec_size never exceeds 128 KiB (LZO_BLOCK_SIZE).
//! Trailing garbage may follow the terminator; the offset of the
//! terminator is the entry's "declared length" in size.grs.
//!
//! Some entries are NOT streams (raw blobs with no valid block walk —
//! e.g. ps2.ico, versiontag.bin). Detection = a clean block walk where
//! every compressed block decodes to exactly its dec_size.

use crate::lzo;

pub const LZO_BLOCK_SIZE: usize = 0x2_0000;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    /// Valid LZO block stream. `terminator` = offset of the dec_size==0
    /// word; `decompressed` = total output bytes (sum of dec_size).
    Lzo { terminator: usize, decompressed: usize },
    /// Not an LZO stream; treat the entry bytes as raw data.
    Raw,
}

fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Walk the block structure without decoding. Returns the terminator
/// offset and total decompressed size if the walk is clean.
pub fn walk_stream(data: &[u8]) -> Option<(usize, usize)> {
    if data.len() < 12 {
        return None;
    }
    if le32(data, 0) as usize != data.len() {
        return None; // bin_file_size must equal the entry length
    }
    let mut off = 4usize;
    let mut total_dec = 0usize;
    loop {
        if off + 4 > data.len() {
            return None;
        }
        let dec = le32(data, off) as usize;
        if dec == 0 {
            return Some((off, total_dec));
        }
        if dec > LZO_BLOCK_SIZE || off + 8 > data.len() {
            return None;
        }
        let comp = le32(data, off + 4) as usize;
        let block_len = if comp >= dec { dec } else { comp };
        if off + 8 + block_len > data.len() {
            return None;
        }
        total_dec += dec;
        off += 8 + block_len;
    }
}

/// Classify an entry, actually decoding every compressed block when
/// `verify` is set (a block that fails to decode disqualifies the
/// stream — some raw blobs walk cleanly by coincidence).
pub fn classify(data: &[u8], verify: bool) -> EntryKind {
    let Some((terminator, total_dec)) = walk_stream(data) else {
        return EntryKind::Raw;
    };
    if verify {
        let mut off = 4usize;
        loop {
            let dec = le32(data, off) as usize;
            if dec == 0 {
                break;
            }
            let comp = le32(data, off + 4) as usize;
            if comp < dec
                && lzo::decompress_block(&data[off + 8..off + 8 + comp], dec).is_err()
            {
                return EntryKind::Raw;
            }
            let block_len = if comp >= dec { dec } else { comp };
            off += 8 + block_len;
        }
    }
    EntryKind::Lzo {
        terminator,
        decompressed: total_dec,
    }
}

/// Fully decompress an entry known (or believed) to be an LZO stream.
pub fn decompress_entry(data: &[u8]) -> Result<Vec<u8>, String> {
    let (terminator, total_dec) =
        walk_stream(data).ok_or_else(|| "not an LZO block stream".to_string())?;
    let _ = terminator;
    let mut out = Vec::with_capacity(total_dec);
    let mut off = 4usize;
    loop {
        let dec = le32(data, off) as usize;
        if dec == 0 {
            break;
        }
        let comp = le32(data, off + 4) as usize;
        if comp >= dec {
            out.extend_from_slice(&data[off + 8..off + 8 + dec]);
            off += 8 + dec;
        } else {
            let block = lzo::decompress_block(&data[off + 8..off + 8 + comp], dec)?;
            out.extend_from_slice(&block);
            off += 8 + comp;
        }
    }
    if out.len() != total_dec {
        return Err(format!(
            "decompressed {} bytes, stream promised {total_dec}",
            out.len()
        ));
    }
    Ok(out)
}

/// size.grs: raw table — u32 total byte length, then (key, declared_len)
/// pairs. NOT itself an LZO stream. Duplicate keys occur (6,655 rows vs
/// 6,642 unique in pop3.bf); the last row wins, matching a plain map
/// build. declared_len = LZO terminator offset for streamed entries.
pub fn parse_size_grs(data: &[u8]) -> std::collections::HashMap<u32, u32> {
    let mut map = std::collections::HashMap::new();
    if data.len() < 4 {
        return map;
    }
    let mut off = 4usize;
    while off + 8 <= data.len() {
        let k = le32(data, off);
        let v = le32(data, off + 4);
        map.insert(k, v);
        off += 8;
    }
    map
}
