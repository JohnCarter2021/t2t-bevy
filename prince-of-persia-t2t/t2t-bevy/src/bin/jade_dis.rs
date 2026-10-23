//! Jade behavior bytecode — first-pass disassembler.
//! A behavior record splits into: header (word+4 =
//! string-pool offset), a cell stream (u32 cells:
//! plain ints, IEEE float bits, and (id, type)
//! references — type 0x0200 variables, 0x1b00/0x1000/
//! 0x1c00 function refs), and a trailing string pool
//! (8-byte header, then NUL-terminated strings).
//! Usage: jade_dis <bf> <record-key-hex> [start-cell]
use std::collections::HashMap;
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
            if i - s >= 4 {
                out.push((s, String::from_utf8_lossy(&d[s..i]).into_owned()));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn fn_name_of(s: &str) -> Option<String> {
    // "Prince->fn_Xxx : ..." / "Bhv_Xxx ..." -> the name.
    for part in s.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        if part.starts_with("fn_") || part.starts_with("fnb_") || part.starts_with("Bhv_") {
            return Some(part.to_string());
        }
    }
    None
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let want = u32::from_str_radix(args[2].trim_start_matches("0x"), 16).expect("key hex");
    let start_cell: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let recs = walk_wow(&data);
    // Global map: header function id -> names from its
    // home record's strings.
    let mut fn_names: HashMap<u16, Vec<String>> = HashMap::new();
    for r in &recs {
        if r.size < 16 {
            continue;
        }
        let w = le32(&data, r.body_off + 12);
        if !matches!(w >> 16, 0x1c00 | 0x1b00) {
            continue;
        }
        let end = (r.body_off + r.size as usize).min(data.len());
        let mut names: Vec<String> = Vec::new();
        for (_, s) in strings_in(&data, r.body_off, end) {
            if let Some(n) = fn_name_of(&s) {
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        if !names.is_empty() {
            fn_names.insert((w & 0xffff) as u16, names);
        }
    }
    let r = recs.iter().find(|r| r.key == want).expect("record");
    let body = r.body_off;
    let end = (body + r.size as usize).min(data.len());
    let pool = body + le32(&data, body + 4) as usize;
    println!(
        "record {:#010x} size {} pool at body+{:#x}",
        r.key,
        r.size,
        le32(&data, body + 4)
    );
    println!(
        "header: {:#010x} {:#010x} {:#010x} {:#010x}",
        le32(&data, body),
        le32(&data, body + 4),
        le32(&data, body + 8),
        le32(&data, body + 12)
    );
    let pool_strings = strings_in(&data, pool, end);
    for (i, (o, s)) in pool_strings.iter().enumerate() {
        println!("  str[{i}] (+{}): {}", o - pool, s.chars().take(72).collect::<String>());
    }
    // Cell listing from body+16 to the pool.
    let ncells = (pool - (body + 16)) / 4;
    println!("{ncells} cells");
    if args.get(4).map(|s| s.as_str()) == Some("pairs") {
        // Pair view: the stream reads as (operand,
        // operator) pairs.
        let fmt = |w: u32| -> String {
            let (id, ty) = ((w & 0xffff) as u16, (w >> 16) as u16);
            match ty {
                0x0000 => format!("{id}"),
                0x3c00..=0x4400 => format!("{}f", f32::from_bits(w)),
                0x0200 => format!("var:{id:x}"),
                0x1b00 | 0x1000 | 0x1c00 => format!("fn:{id:x}"),
                0x0300 => format!("r3:{id:x}"),
                _ => format!("{id:x}.{ty:x}"),
            }
        };
        let mut idx = start_cell;
        while idx + 1 < ncells + 1 && idx < ncells {
            let a = le32(&data, body + 16 + idx * 4);
            let b = if idx + 1 < ncells {
                le32(&data, body + 16 + (idx + 1) * 4)
            } else {
                0
            };
            println!("{idx:5}: ({}, {})", fmt(a), fmt(b));
            idx += 2;
        }
        return ExitCode::SUCCESS;
    }
    let mut idx = start_cell;
    while idx < ncells {
        let mut line = format!("{idx:5}:");
        for j in 0..8 {
            if idx + j >= ncells {
                break;
            }
            let w = le32(&data, body + 16 + (idx + j) * 4);
            let (id, ty) = ((w & 0xffff) as u16, (w >> 16) as u16);
            let tok = match ty {
                0x0000 => format!("{id}"),
                0x3c00..=0x4400 => format!("{}f", f32::from_bits(w)),
                0x0200 => format!("var:{id:x}"),
                0x1b00 | 0x1000 | 0x1c00 => {
                    let tag = match ty {
                        0x1b00 => "fnD",
                        0x1000 => "fnC",
                        _ => "fnH",
                    };
                    match fn_names.get(&id) {
                        Some(ns) => format!("{tag}:{}:{}", id, ns.join("|")),
                        None => format!("{tag}:{id:x}"),
                    }
                }
                0x0300 => format!("r3:{id:x}"),
                _ => format!("{id:x}.{ty:x}"),
            };
            line.push_str(&format!(" {tok}"));
        }
        println!("{line}");
        idx += 8;
    }
    ExitCode::SUCCESS
}
