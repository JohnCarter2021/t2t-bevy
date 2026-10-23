//! Behavior-record anatomy: header hex, string table,
//! and reference sites for a chosen string (u32 offsets
//! pointing at it), with context dumps.
//! Usage: bhv_dump <bf> <record-key-hex> <string-substr>
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn hexdump(d: &[u8], start: usize, len: usize, base: usize) {
    for row in (0..len).step_by(16) {
        let o = start + row;
        if o >= d.len() {
            break;
        }
        let bytes: Vec<String> = (0..16)
            .filter(|&i| o + i < d.len() && i < len - row)
            .map(|i| format!("{:02x}", d[o + i]))
            .collect();
        let ascii: String = (0..16)
            .filter(|&i| o + i < d.len() && i < len - row)
            .map(|i| {
                let c = d[o + i];
                if (0x20..0x7f).contains(&c) {
                    c as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("  {:5} {}  {}", o - base, bytes.join(" "), ascii);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let want = u32::from_str_radix(args[2].trim_start_matches("0x"), 16).expect("key hex");
    let needle = &args[3];
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let recs = walk_wow(&data);
    let r = recs.iter().find(|r| r.key == want).expect("record");
    let body = r.body_off;
    let end = (body + r.size as usize + 12).min(data.len());
    println!(
        "record {:#010x} size {} body_off {:#x} (stream {:#x}..{:#x})",
        r.key, r.size, body, r.offset, end
    );
    println!("header (first 192 bytes from body_off):");
    hexdump(&data, body, 192, body);
    // Strings in this record.
    let mut strings: Vec<(usize, String)> = Vec::new();
    let mut i = body;
    while i < end {
        if (0x20..0x7f).contains(&data[i]) {
            let s = i;
            while i < end && (0x20..0x7f).contains(&data[i]) {
                i += 1;
            }
            if i - s >= 4 {
                strings.push((
                    s - body,
                    String::from_utf8_lossy(&data[s..i]).into_owned(),
                ));
            }
        } else {
            i += 1;
        }
    }
    println!("{} strings in record", strings.len());
    // Target string + reference sites.
    for (off, s) in &strings {
        if s.contains(needle.as_str()) {
            println!("target at body+{off} ({off:#x}): {s}");
            for base_name in ["body", "recstart"] {
                let target = if base_name == "body" {
                    *off as u32
                } else {
                    (off + 12) as u32
                };
                let mut sites = Vec::new();
                let mut o = body;
                while o + 4 <= end {
                    if le32(&data, o) == target {
                        sites.push(o - body);
                    }
                    o += 1;
                }
                println!("  refs as {base_name}-relative ({target:#x}): {:?}", sites);
                for site in sites.iter().take(3) {
                    println!("  context around body+{site}:");
                    hexdump(&data, body + site.saturating_sub(32), 96, body);
                }
            }
        }
    }
    ExitCode::SUCCESS
}
