//! scan_anims — rank sectors by animation content: clip
//! records (gro 0x2002b/0x2002c) and descriptor records
//! (52 B payload, type word low half 0xFF01).
//!
//! Usage: scan_anims <pop3.bf> [top-n]

use std::collections::BTreeMap;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: scan_anims <pop3.bf> [top]");
    let top: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(15);
    // Detail mode: SCAN_DETAIL=<sector-substr> resolves the
    // sector's descriptor list against its own clip records.
    if let Ok(want) = std::env::var("SCAN_DETAIL") {
        let bf = BigFile::open(&bf_path).expect("open bf");
        let e = bf.active_files().into_iter().find(|e| e.name.contains(&want)).expect("sector");
        let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
        let records = walk_wow(&data);
        let clips: std::collections::HashMap<u32, usize> = records
            .iter()
            .filter(|r| r.gro_type == Some(0x2002b))
            .map(|r| (r.key, r.data_range().len()))
            .collect();
        println!("sector {}: {} clips", e.name, clips.len());
        for (i, r) in records.iter().enumerate() {
            if r.size == 52 {
                if let Some(gt) = r.gro_type {
                    if gt & 0xFFFF == 0xFF01 {
                        let low = gt >> 16;
                        let hit = clips.iter().find(|(k, _)| **k & 0xFFFF == low as u32);
                        match hit {
                            Some((k, sz)) => println!("  desc[{i}] -> clip {k:#010x} ({sz} B)"),
                            None => println!("  desc[{i}] -> low16 {low:#06x} (not in sector)"),
                        }
                    }
                }
            }
        }
        return;
    }
    let bf = BigFile::open(&bf_path).expect("open bf");
    let mut rows: Vec<(String, usize, usize, usize, Vec<(u32, f32, u32)>)> = Vec::new();
    for e in bf.active_files() {
        if !e.name.contains("_wow_") {
            continue;
        }
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        let records = walk_wow(&data);
        let mut clips = 0usize;
        let mut clip_bytes = 0usize;
        let mut descs: Vec<(u32, f32, u32)> = Vec::new();
        for r in &records {
            let d = &data[r.data_range()];
            match r.gro_type {
                Some(0x2002b) | Some(0x2002c) => {
                    clips += 1;
                    clip_bytes += d.len();
                }
                _ => {}
            }
            if r.size == 52 {
                if let Some(gt) = r.gro_type {
                    if gt & 0xFFFF == 0xFF01 {
                        // Descriptor: type word carries the data
                        // key's low 16 bits; data_range excludes
                        // the type word (f32 at +30, tail at +36).
                        let data_low = gt >> 16;
                        let dur = f32::from_le_bytes(d[30..34].try_into().unwrap());
                        let tail = u32le(d, 36);
                        descs.push((data_low, dur, tail));
                    }
                }
            }
        }
        if clips > 0 || !descs.is_empty() {
            rows.push((e.name.clone(), clips, clip_bytes, descs.len(), descs));
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));
    let mut by_sector: BTreeMap<String, ()> = BTreeMap::new();
    for (name, clips, bytes, ndesc, descs) in rows.iter().take(top) {
        println!("{name}: {clips} clips, {bytes} clip bytes, {ndesc} descriptors");
        for (k, dur, tail) in descs.iter().take(8) {
            println!("    desc -> data low16 {k:#06x} dur {dur:.1} header {tail:#010x}");
        }
        by_sector.insert(name.clone(), ());
    }
    println!("sectors with any anim content: {}", rows.len());
}
