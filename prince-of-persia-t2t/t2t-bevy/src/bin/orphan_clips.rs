//! orphan_clips — clip-data records that NOTHING references:
//! no descriptor anywhere points at them, and no record in
//! any sector contains their key as an aligned u32. The
//! Prince's base locomotion, bound by an undecoded
//! mechanism, must be data somewhere — orphans are the pool.
//!
//! Usage: orphan_clips <pop3.bf> <out-keys-file>

use std::collections::{HashMap, HashSet};
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: orphan_clips <bf> <out>");
    let out_path = std::env::args().nth(2).expect("usage");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let mut clip_home: HashMap<u32, String> = HashMap::new();
    let mut referenced: HashSet<u32> = HashSet::new();
    let mut sector_data: Vec<(String, Vec<u8>)> = Vec::new();
    for e in bf.active_files() {
        if !e.name.contains("_wow_") {
            continue;
        }
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        for r in walk_wow(&data) {
            if matches!(r.gro_type, Some(0x20025) | Some(0x2002b) | Some(0x2002c)) {
                clip_home.entry(r.key).or_insert_with(|| e.name.clone());
            }
            // Descriptor references.
            if r.size == 52 {
                if let Some(gt) = r.gro_type {
                    if gt & 0xFFFF == 0xFF01 {
                        let d = &data[r.data_range()];
                        let hi = u32::from_le_bytes(d[0..4].try_into().unwrap());
                        referenced.insert((hi << 16) | (gt >> 16));
                    }
                }
            }
        }
        sector_data.push((e.name.clone(), data));
    }
    // Aligned-u32 references from every record body.
    let clip_keys: HashSet<u32> = clip_home.keys().cloned().collect();
    for (_, data) in &sector_data {
        for r in walk_wow(data) {
            let body = &data[r.body_off..r.body_off + r.size as usize];
            if body.len() < 4 {
                continue;
            }
            for off in (0..body.len() - 3).step_by(4) {
                let v = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                if clip_keys.contains(&v) {
                    referenced.insert(v);
                }
            }
        }
    }
    let mut orphans: Vec<u32> = clip_home
        .keys()
        .filter(|k| !referenced.contains(k))
        .cloned()
        .collect();
    orphans.sort();
    println!("clip keys: {}, referenced: {}, orphans: {}", clip_home.len(), referenced.len(), orphans.len());
    let mut out = String::new();
    for k in &orphans {
        out.push_str(&format!("{k:#010x}\n"));
    }
    std::fs::write(&out_path, out).expect("write keys");
    // Where do orphans live? (sector histogram, top 12)
    let mut hist: HashMap<String, usize> = HashMap::new();
    for k in &orphans {
        *hist.entry(clip_home[k].clone()).or_default() += 1;
    }
    let mut hv: Vec<_> = hist.into_iter().collect();
    hv.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (sec, n) in hv.iter().take(12) {
        println!("  {n:3} orphans in {}", sec.split("_wow_").next().unwrap_or(sec));
    }
}
