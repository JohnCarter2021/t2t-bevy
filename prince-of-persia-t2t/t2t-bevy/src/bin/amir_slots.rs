//! Slot grammar probe: for every AMIR_wow record,
//! test every aligned u32 slot (from body_off) against
//! the archive clip-key set and tally hits per offset.
//! Offsets that hit repeatedly are key-bearing fields.
use std::collections::BTreeSet;
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let clip_keys: BTreeSet<u32> = std::fs::read_to_string("out/clip_keys.txt")
        .expect("clip_keys.txt")
        .lines()
        .filter_map(|l| u32::from_str_radix(l.trim(), 16).ok())
        .collect();
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let recs = walk_wow(&data);
    // hits per offset (relative to body_off), and the
    // record sizes that produced them.
    let mut tally: std::collections::HashMap<usize, Vec<u32>> = std::collections::HashMap::new();
    for r in &recs {
        let end = (r.body_off + r.size as usize + 12).min(data.len());
        let mut off = r.body_off;
        while off + 4 <= end {
            let v = le32(&data, off);
            if clip_keys.contains(&v) {
                tally.entry(off - r.body_off).or_default().push(v);
            }
            off += 4;
        }
    }
    let mut offs: Vec<usize> = tally.keys().copied().collect();
    offs.sort();
    for o in offs {
        let hits = &tally[&o];
        if hits.len() >= 2 {
            let mut uniq: Vec<u32> = hits.clone();
            uniq.sort();
            uniq.dedup();
            println!(
                "offset +{o:3}: {} hits, {} unique (e.g. {:#010x})",
                hits.len(),
                uniq.len(),
                uniq[0]
            );
        }
    }
    println!("records scanned: {}", recs.len());
    ExitCode::SUCCESS
}
