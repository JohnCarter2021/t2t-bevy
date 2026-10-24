//! Hunt for the event->sound link inside pop3.bf:
//! sweep every decompressed wow stream for (a)
//! printable strings mentioning sound/audio/dare,
//! (b) records containing BOTH a Jade event id
//! (0x0017xxxx family) and a DARE sound id
//! (0x06/0x08/0x09/0x0a families) as u32 cells.
//!
//! snd_hunt <pop3.bf>

use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn strings_in(data: &[u8]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if (0x20..0x7f).contains(&data[i]) {
            let start = i;
            while i < data.len() && (0x20..0x7f).contains(&data[i]) {
                i += 1;
            }
            if i - start >= 4 {
                out.push((start, String::from_utf8_lossy(&data[start..i]).into_owned()));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn u32s(data: &[u8]) -> impl Iterator<Item = u32> + '_ {
    data.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: snd_hunt <pop3.bf> [dump <substr>]");
    let bf = BigFile::open(&path).expect("open bf");
    // record mode: every u32 cell of one record.
    if std::env::args().nth(2).as_deref() == Some("record") {
        let sub = std::env::args().nth(3).expect("file substring");
        let key = u32::from_str_radix(std::env::args().nth(4).expect("key hex").trim_start_matches("0x"), 16).unwrap();
        for f in bf.active_files() {
            if !f.name.contains(&sub) {
                continue;
            }
            let raw = bf.read_data(f.index);
            let data = match decompress_entry(raw) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for r in walk_wow(&data) {
                if r.key != key {
                    continue;
                }
                let end = (r.offset + r.size as usize).min(data.len());
                let body = &data[r.body_off..end];
                println!("== {} rec {:#x} size {} body {} bytes", f.name, r.key, r.size, body.len());
                for (i, c) in body.chunks_exact(4).enumerate() {
                    let v = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    let f32v = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    println!("  +{:4} {:#010x}  f32 {:>12.4}  id {:#06x} type {:#06x}", i * 4, v, f32v, v & 0xffff, v >> 16);
                }
                for (off, s) in strings_in(body) {
                    println!("  str @{}: {:?}", off, s);
                }
            }
        }
        return;
    }
    // dump mode: full record listing for files whose
    // name contains the substring.
    if std::env::args().nth(2).as_deref() == Some("dump") {
        let sub = std::env::args().nth(3).expect("dump needs a substring");
        for f in bf.active_files() {
            if !f.name.contains(&sub) {
                continue;
            }
            let raw = bf.read_data(f.index);
            let data = match decompress_entry(raw) {
                Ok(d) => d,
                Err(_) => continue,
            };
            println!("== {} ({} records)", f.name, walk_wow(&data).len());
            for r in walk_wow(&data) {
                let end = (r.offset + r.size as usize).min(data.len());
                let body = if r.body_off <= end { &data[r.body_off..end] } else { &[][..] };
                let strs: Vec<String> = strings_in(body).into_iter().map(|(_, s)| s).collect();
                let cells: Vec<String> = u32s(body).take(10).map(|v| format!("{v:#x}")).collect();
                println!(
                    "  rec {:#010x} size {} tag={:?} gro={:?} cells[{}] strs{:?}",
                    r.key, r.size, r.tag, r.gro_type, cells.join(","), strs
                );
            }
        }
        return;
    }
    let mut snd_strings = 0usize;
    let mut cooccur = 0usize;
    for f in bf.active_files() {
        let raw = bf.read_data(f.index);
        if raw.is_empty() {
            continue;
        }
        let data = match decompress_entry(&raw) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let records = walk_wow(&data);
        for r in &records {
            let end = (r.offset + r.size as usize).min(data.len());
            if r.body_off > end {
                continue;
            }
            let body = &data[r.body_off..end];
            // (a) sound-ish strings
            for (off, s) in strings_in(body) {
                let low = s.to_ascii_lowercase();
                if low.contains("sound") || low.contains("snd") || low.contains("dare") || low.contains("audio") {
                    println!("STR {} rec {:#010x} @{}: {}", f.name, r.key, off, s);
                    snd_strings += 1;
                }
            }
            // (b) co-occurrence of event id + sound id
            let mut ev: Vec<u32> = Vec::new();
            let mut sd: Vec<u32> = Vec::new();
            for v in u32s(body) {
                match v >> 16 {
                    0x0017 => ev.push(v),
                    0x0006 | 0x0008 | 0x0009 | 0x000a => sd.push(v),
                    _ => {}
                }
            }
            if !ev.is_empty() && !sd.is_empty() {
                println!(
                    "COOCCUR {} rec {:#010x} tag={:?} events={:x?} sounds={:x?}",
                    f.name, r.key, r.tag, &ev[..ev.len().min(4)], &sd[..sd.len().min(4)]
                );
                cooccur += 1;
            }
        }
    }
    println!("done: {} sound strings, {} co-occurring records", snd_strings, cooccur);
}
