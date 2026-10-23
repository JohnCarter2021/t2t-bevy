//! Dump every clip-container key in the archive
//! (gro 0x20025 / 0x2002b / 0x2002c), one hex key per
//! line, for cross-referencing against other sources
//! (e.g. tables inside pop3.exe).
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let mut keys: Vec<u32> = Vec::new();
    for e in bf.active_files() {
        if let Ok(data) = decompress_entry(bf.read_data(e.index)) {
            for r in walk_wow(&data) {
                if matches!(r.gro_type, Some(0x20025) | Some(0x2002b) | Some(0x2002c)) {
                    keys.push(r.key);
                }
            }
        }
    }
    keys.sort();
    keys.dedup();
    for k in &keys {
        println!("{k:08x}");
    }
    eprintln!("{} unique clip keys", keys.len());
    ExitCode::SUCCESS
}
