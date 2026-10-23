//! Dump one AMIR_wow record's descriptor references in
//! offset order, each resolved descriptor -> clip key
//! -> frame count. Usage: amir_refs <bf> <record key hex>
use std::collections::BTreeSet;
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::Clip;
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let want: u32 = u32::from_str_radix(args[2].trim_start_matches("0x"), 16).expect("key hex");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let recs = walk_wow(&data);
    let by_key: std::collections::HashMap<u32, usize> =
        recs.iter().enumerate().map(|(i, r)| (r.key, i)).collect();

    let ri = by_key[&want];
    let r = &recs[ri];
    println!(
        "record {:#010x}: size {} body_off {:#x}",
        r.key, r.size, r.body_off
    );
    let end = (r.body_off + r.size as usize + 12).min(data.len());
    let mut off = r.body_off;
    let mut n = 0;
    while off + 4 <= end {
        let v = le32(&data, off);
        if let Some(di) = by_key.get(&v) {
            let d = &recs[*di];
            // Is it a descriptor? Resolve its clip.
            let ty = le32(&data, d.body_off);
            if d.data_range().len() >= 40 && ty & 0xffff == 0xff01 {
                let hi = le32(&data, d.data_range().start);
                let clip = (hi << 16) | (ty >> 16);
                // Frames: the clip record may live in
                // this sector or elsewhere; try here.
                let frames = by_key.get(&clip).map(|ci| {
                    let cr = &recs[*ci];
                    Clip::load(&data[cr.data_range()]).max_frame
                });
                println!(
                    "  +{:5}: desc {:#010x} -> clip {:#010x} frames_here {:?}",
                    off - r.body_off,
                    v,
                    clip,
                    frames
                );
                n += 1;
            }
        }
        off += 4;
    }
    println!("{n} descriptor refs");
    let _ = BTreeSet::<u32>::new();
    ExitCode::SUCCESS
}
