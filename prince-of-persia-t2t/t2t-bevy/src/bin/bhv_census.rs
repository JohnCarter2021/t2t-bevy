//! Census of AMIR behavior records: gro-0 records with
//! fn_/Bhv_/Prince-> strings; header words side by side
//! and string-pool tails.
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn strings_in(d: &[u8], start: usize, end: usize) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = start;
    while i < end {
        if (0x20..0x7f).contains(&d[i]) {
            let s = i;
            while i < end && (0x20..0x7f).contains(&d[i]) {
                i += 1;
            }
            if i - s >= 5 {
                out.push((s, String::from_utf8_lossy(&d[s..i]).into_owned()));
            }
        } else {
            i += 1;
        }
    }
    out
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
    let mut hits = 0;
    for r in &recs {
        let end = (r.body_off + r.size as usize).min(data.len());
        let strs = strings_in(&data, r.body_off, end);
        let interesting = strs.iter().any(|(_, s)| {
            s.contains("fn_") || s.contains("Bhv_") || s.contains("Prince->") || s.contains("->")
        });
        if !interesting && r.size < 4000 {
            continue;
        }
        if !interesting {
            continue;
        }
        hits += 1;
        let w: Vec<String> = (0..8).map(|i| format!("{:#010x}", le32(&data, r.body_off + i * 4))).collect();
        println!(
            "key {:#010x} size {:6} gro {:?} hdr {}",
            r.key,
            r.size,
            r.gro_type,
            w.join(" ")
        );
        for (o, s) in strs.iter().take(6) {
            println!("    str +{:6}: {}", o - r.body_off, s.chars().take(70).collect::<String>());
        }
        if strs.len() > 6 {
            println!("    ... {} strings total", strs.len());
        }
    }
    println!("{hits} behavior-ish records");
    ExitCode::SUCCESS
}
