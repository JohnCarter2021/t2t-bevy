//! Anatomy dumps for the GAM decode: (1) the bank
//! record's descriptor-ref layout with gap analysis;
//! (2) full 64-byte hex of chosen descriptor records
//! (args: descriptor keys, hex, comma-separated).
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
    let is_desc = |k: u32| -> bool {
        by_key.get(&k).map_or(false, |&i| {
            let r = &recs[i];
            r.data_range().len() >= 40 && le32(&data, r.body_off) & 0xffff == 0xff01
        })
    };
    // (1) Bank layout.
    let bank = &recs[by_key[&0x2d000acc]];
    println!(
        "bank record: size {} body_off {:#x} gro {:?}",
        bank.size, bank.body_off, bank.gro_type
    );
    let end = (bank.body_off + bank.size as usize + 12).min(data.len());
    let mut refs: Vec<usize> = Vec::new();
    let mut off = bank.body_off;
    while off + 4 <= end {
        if is_desc(le32(&data, off)) {
            refs.push(off - bank.body_off);
        }
        off += 4;
    }
    println!("{} refs; gaps between consecutive refs:", refs.len());
    let mut gaps: Vec<(usize, usize)> = Vec::new(); // (gap, at ref idx)
    for i in 1..refs.len() {
        gaps.push((refs[i] - refs[i - 1], i));
    }
    let mut hist: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (g, _) in &gaps {
        *hist.entry(*g).or_default() += 1;
    }
    let mut hv: Vec<(usize, usize)> = hist.into_iter().collect();
    hv.sort();
    println!("gap histogram (bytes: count): {:?}", hv);
    println!(
        "refs NOT 4 bytes after the previous (group breaks): {:?}",
        gaps.iter().filter(|(g, _)| *g != 4).collect::<Vec<_>>()
    );
    println!(
        "first ref at +{}, last ref at +{}, record payload end {}",
        refs.first().copied().unwrap_or(0),
        refs.last().copied().unwrap_or(0),
        bank.size + 12
    );
    // (2) Descriptor hex dumps.
    if args.len() > 2 {
        for kstr in args[2].split(',') {
            let k = u32::from_str_radix(kstr.trim().trim_start_matches("0x"), 16).expect("desc hex");
            let r = &recs[by_key[&k]];
            let start = r.body_off;
            println!("\ndescriptor {k:#010x} size {} body_off {start:#x}:", r.size);
            for row in 0..4 {
                let o = start + row * 16;
                let bytes: Vec<String> = (0..16).map(|i| format!("{:02x}", data[o + i])).collect();
                let words: Vec<String> = (0..4)
                    .map(|i| format!("{:#010x}", le32(&data, o + i * 4)))
                    .collect();
                println!("  +{:2} {} | {}", row * 16, bytes.join(" "), words.join(" "));
            }
        }
    }
    ExitCode::SUCCESS
}
