//! amir_xref — find AMIR_wow's action-table records: scan
//! every AMIR record's payload for u32 values that are keys
//! of animation-family records somewhere in the archive.
//! Hits on keys OUTSIDE the descriptor set are the prize
//! (movement actions reference their clips directly).
//!
//! Usage: amir_xref <pop3.bf>

use std::collections::{HashMap, HashSet};
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn load_sector(bf: &BigFile, substr: &str) -> Option<Vec<u8>> {
    let e = bf.active_files().into_iter().find(|e| e.name.contains(substr))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: amir_xref <bf>");
    let bf = BigFile::open(&bf_path).expect("open bf");
    // Global anim-family key set + the descriptor set.
    let mut anim_keys: HashSet<u32> = HashSet::new();
    let mut all_keys: HashMap<u32, Option<u32>> = HashMap::new();
    for e in bf.active_files() {
        if !e.name.contains("_wow_") {
            continue;
        }
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        for r in walk_wow(&data) {
            all_keys.entry(r.key).or_insert(r.gro_type);
            // Only true clip-data containers count as targets.
            if matches!(r.gro_type, Some(0x20025) | Some(0x2002b) | Some(0x2002c)) {
                anim_keys.insert(r.key);
            }
        }
    }
    let sector = std::env::args().nth(2).unwrap_or_else(|| "AMIR_wow".to_string());
    let amir = load_sector(&bf, &sector).expect("sector");
    let records = walk_wow(&amir);
    // Descriptor-referenced keys (the known set) — from AMIR,
    // the Prince's home list, regardless of target sector.
    let amir_home = load_sector(&bf, "AMIR_wow").expect("amir home");
    let mut desc_keys: HashSet<u32> = HashSet::new();
    for r in &walk_wow(&amir_home) {
        if r.size == 52 {
            if let Some(gt) = r.gro_type {
                if gt & 0xFFFF == 0xFF01 {
                    let d = &amir_home[r.data_range()];
                    let hi = u32::from_le_bytes(d[0..4].try_into().unwrap());
                    desc_keys.insert((hi << 16) | (gt >> 16));
                }
            }
        }
    }
    println!("descriptor set: {} keys; anim-family keys archive-wide: {}", desc_keys.len(), anim_keys.len());
    for (i, r) in records.iter().enumerate() {
        let body = &amir[r.body_off..r.body_off + r.size as usize];
        if body.len() < 8 {
            continue;
        }
        let mut hits: Vec<(usize, u32, bool)> = Vec::new(); // (offset, key, is_new)
        for off in (0..body.len() - 3).step_by(4) {
            let v = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
            if anim_keys.contains(&v) {
                hits.push((off, v, !desc_keys.contains(&v)));
            }
        }
        if hits.is_empty() {
            continue;
        }
        // Dedupe consecutive duplicates.
        hits.dedup_by(|a, b| a.1 == b.1 && a.0 <= b.0 + 4);
        let new_count = hits.iter().filter(|h| h.2).count();
        println!(
            "rec[{i}] key {:#010x} gro {:#x?} ({} B): {} anim refs, {} NEW",
            r.key,
            r.gro_type,
            body.len(),
            hits.len(),
            new_count
        );
        for (off, k, is_new) in &hits {
            println!("    @{off}: {k:#010x}{}", if *is_new { " NEW" } else { "" });
        }
    }
}
