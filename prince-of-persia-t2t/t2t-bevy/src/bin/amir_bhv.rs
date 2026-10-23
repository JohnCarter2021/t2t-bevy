//! Behavior-record forensics for AMIR_wow: locate the
//! record containing a given stream offset, then scan
//! AMIR's records for references to DESCRIPTOR record
//! keys (indirection) as well as direct clip keys, with
//! per-offset tallies.
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
    let target: usize = usize::from_str_radix(args[2].trim_start_matches("0x"), 16).expect("offset hex");
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

    // Descriptor records in AMIR (standard signature).
    let mut desc_keys: BTreeSet<u32> = BTreeSet::new();
    for r in &recs {
        if r.data_range().len() >= 40 && le32(&data, r.body_off) & 0xffff == 0xff01 {
            desc_keys.insert(r.key);
        }
    }
    println!("{} descriptor records in AMIR", desc_keys.len());

    // Which record holds the target offset?
    for (i, r) in recs.iter().enumerate() {
        let start = r.offset;
        let end = start + r.size as usize + 12;
        if target >= start && target < end {
            println!(
                "target {target:#x} is in record #{i}: key {:#010x} gro {:?} size {} body_off {:#x} (record spans {start:#x}..{end:#x})",
                r.key, r.gro_type, r.size, r.body_off
            );
            // Dump this record's slots vs both key sets.
            let mut off = r.body_off;
            while off + 4 <= end.min(data.len()) {
                let v = le32(&data, off);
                if clip_keys.contains(&v) {
                    println!("  +{:4}: CLIP key {v:#010x}", off - r.body_off);
                } else if desc_keys.contains(&v) {
                    println!("  +{:4}: DESCRIPTOR record key {v:#010x}", off - r.body_off);
                }
                off += 4;
            }
        }
    }

    // Global indirection scan: aligned u32 == descriptor
    // record key, tallied by offset from body_off.
    let mut tally: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for r in &recs {
        let end = (r.body_off + r.size as usize + 12).min(data.len());
        let mut off = r.body_off;
        while off + 4 <= end {
            if desc_keys.contains(&le32(&data, off)) {
                *tally.entry(off - r.body_off).or_default() += 1;
            }
            off += 4;
        }
    }
    let mut offs: Vec<usize> = tally.keys().copied().collect();
    offs.sort();
    for o in offs {
        if tally[&o] >= 2 {
            println!("descriptor-key refs at +{o}: {} hits", tally[&o]);
        }
    }
    ExitCode::SUCCESS
}
