//! Resolve AMIR_wow's EXTENDED descriptors: every
//! record gets its body_off+52 slot tested against the
//! archive-wide clip-key set. Compares the result with
//! the standard descriptors' clip set (the known 92) and
//! lists any NEW resolved clips with frame counts.
use std::collections::BTreeSet;
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::Clip;
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn load_sector(bf: &BigFile, substr: &str) -> Option<Vec<u8>> {
    let e = bf.active_files().into_iter().find(|e| e.name.contains(substr))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    // Archive-wide clip-key set + a key -> (sector data is
    // too heavy to keep; resolve frames later per sector).
    let clip_keys: BTreeSet<u32> = std::fs::read_to_string("out/clip_keys.txt")
        .expect("clip_keys.txt (run clip_keys first)")
        .lines()
        .filter_map(|l| u32::from_str_radix(l.trim(), 16).ok())
        .collect();
    let data = load_sector(&bf, "AMIR_wow").expect("amir sector");
    let recs = walk_wow(&data);

    // Standard descriptors -> the known set.
    let mut standard: BTreeSet<u32> = BTreeSet::new();
    for r in &recs {
        let dr = r.data_range();
        if dr.end <= data.len() && dr.len() >= 40 {
            let ty = le32(&data, r.body_off);
            if ty & 0xffff == 0xff01 {
                let hi = le32(&data, dr.start);
                standard.insert((hi << 16) | (ty >> 16));
            }
        }
    }
    println!("standard descriptors resolve {} clips", standard.len());

    // Extended slot probe on every record.
    let mut ext_hits: Vec<(u32, u32, u32)> = Vec::new(); // (record key, size, clip key)
    for r in &recs {
        let at = r.body_off + 52;
        if at + 4 <= data.len() {
            let v = le32(&data, at);
            if clip_keys.contains(&v) {
                ext_hits.push((r.key, r.size, v));
            }
        }
    }
    let ext_set: BTreeSet<u32> = ext_hits.iter().map(|h| h.2).collect();
    println!(
        "{} records carry a clip key at body_off+52 -> {} unique clips",
        ext_hits.len(),
        ext_set.len()
    );
    let new_keys: Vec<u32> = ext_set.difference(&standard).copied().collect();
    println!("{} of those are NOT in the standard set:", new_keys.len());
    // Frame counts: find each new key's clip record
    // archive-wide (first sector that holds it wins).
    for k in &new_keys {
        let mut frames = None;
        for name in ["AMIR_wow", "Char_wow", "PrinceFinal_Shape", "100_Quai"] {
            if let Some(d) = load_sector(&bf, name) {
                if let Some(r) = walk_wow(&d).iter().find(|r| r.key == *k) {
                    frames = Some(Clip::load(&d[r.data_range()]).max_frame);
                    break;
                }
            }
        }
        println!("  new clip {k:#010x} frames {:?}", frames);
    }
    // Record-size histogram of the extended hits.
    let mut sizes: Vec<u32> = ext_hits.iter().map(|h| h.1).collect();
    sizes.sort();
    sizes.dedup();
    println!("extended-hit record sizes: {:?}", sizes);
    ExitCode::SUCCESS
}
