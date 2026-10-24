//! Targeted string sweep: print strings matching the given
//! case-insensitive substrings, with file + record context.
//! usage: str_hunt <pop3.bf> <substr1> [substr2...]
use std::process::ExitCode;
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let pats: Vec<String> = args[2..].iter().map(|s| s.to_ascii_lowercase()).collect();
    let bf = BigFile::open(&args[1]).expect("open bf");
    for f in bf.active_files() {
        let raw = bf.read_data(f.index);
        if raw.is_empty() {
            continue;
        }
        let data = match decompress_entry(&raw) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for r in walk_wow(&data) {
            let end = (r.offset + r.size as usize).min(data.len());
            if r.body_off > end {
                continue;
            }
            let body = &data[r.body_off..end];
            for (off, s) in strings_in(body) {
                let low = s.to_ascii_lowercase();
                if pats.iter().any(|p| low.contains(p.as_str())) {
                    println!("{} rec {:#010x} @{}: {}", f.name, r.key, off, s);
                }
            }
        }
    }
    ExitCode::SUCCESS
}
