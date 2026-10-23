//! anim_resolve — archive-wide animation resolution: build
//! a global index of clip records (gro 0x2002b), then resolve
//! one sector's descriptor list against it.
//!
//! Usage: anim_resolve <pop3.bf> <sector-substr>

use std::collections::HashMap;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: anim_resolve <bf> <sector>");
    let want = std::env::args().nth(2).expect("usage");
    let bf = BigFile::open(&bf_path).expect("open bf");
    // Global record index: full key -> (sector, gro, bytes).
    let mut index: HashMap<u32, Vec<(String, Option<u32>, usize)>> = HashMap::new();
    let mut sectors: Vec<(String, Vec<u8>)> = Vec::new();
    for e in bf.active_files() {
        if !e.name.contains("_wow_") {
            continue;
        }
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        for r in walk_wow(&data) {
            if (r.gro_type.unwrap_or(0) >> 16) == 0x0002 || r.gro_type == Some(0x2002b) {
                index
                    .entry(r.key)
                    .or_default()
                    .push((e.name.clone(), r.gro_type, r.data_range().len()));
            }
        }
        if e.name.contains(&want) {
            sectors.push((e.name.clone(), data));
        }
    }
    println!("global anim-family records indexed: {}", index.len());
    for (name, data) in &sectors {
        println!("== descriptors of {name}");
        for (i, r) in walk_wow(data).iter().enumerate() {
            if r.size == 52 {
                if let Some(gt) = r.gro_type {
                    if gt & 0xFFFF == 0xFF01 {
                        // Full referenced key: high half stored
                        // at data_range +0 (payload +4), low
                        // half in the type word's high half.
                        let d = &data[r.data_range()];
                        let hi = u32::from_le_bytes(d[0..4].try_into().unwrap());
                        let full = (hi << 16) | (gt >> 16);
                        match index.get(&full) {
                            Some(v) => {
                                for (sec, gro, sz) in v {
                                    let short = sec.split("_wow_").next().unwrap_or(sec);
                                    println!("  desc[{i}] -> {full:#010x} gro {gro:#x?} in {short} ({sz} B)");
                                }
                            }
                            None => println!("  desc[{i}] -> {full:#010x} NOT an anim record anywhere"),
                        }
                    }
                }
            }
        }
    }
}
