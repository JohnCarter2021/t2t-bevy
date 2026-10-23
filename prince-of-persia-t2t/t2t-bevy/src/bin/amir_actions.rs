//! Enumerate AMIR_wow records that reference
//! descriptor records (the action records), resolving
//! each reference to its clip. Records with >= 3 refs
//! are printed in full.
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
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let recs = walk_wow(&data);
    let by_key: std::collections::HashMap<u32, usize> =
        recs.iter().enumerate().map(|(i, r)| (r.key, i)).collect();
    // Resolve descriptor record key -> clip key.
    let resolve = |dk: u32| -> Option<u32> {
        let d = &recs[*by_key.get(&dk)?];
        let ty = le32(&data, d.body_off);
        if d.data_range().len() >= 40 && ty & 0xffff == 0xff01 {
            Some((le32(&data, d.data_range().start) << 16) | (ty >> 16))
        } else {
            None
        }
    };
    let mut n_actions = 0;
    for r in &recs {
        let end = (r.body_off + r.size as usize + 12).min(data.len());
        let mut refs: Vec<(usize, u32, u32)> = Vec::new();
        let mut off = r.body_off;
        while off + 4 <= end {
            let v = le32(&data, off);
            if let Some(clip) = resolve(v) {
                refs.push((off - r.body_off, v, clip));
            }
            off += 4;
        }
        if !refs.is_empty() {
            n_actions += 1;
        }
        if refs.len() >= 3 {
            println!(
                "record {:#010x} gro {:?} size {}: {} refs",
                r.key,
                r.gro_type,
                r.size,
                refs.len()
            );
            for (o, dk, ck) in &refs {
                println!("    +{o:4}: desc {dk:#010x} -> clip {ck:#010x}");
            }
        }
    }
    println!("records with >=1 descriptor ref: {n_actions}");
    ExitCode::SUCCESS
}
