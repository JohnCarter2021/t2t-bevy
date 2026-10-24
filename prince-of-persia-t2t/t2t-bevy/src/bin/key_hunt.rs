//! Debug: find every wow entry carrying a given record key
//! with more than header-only data.
//! usage: key_hunt <bf> <key-hex>...
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let keys: Vec<u32> = args[2..]
        .iter()
        .map(|k| u32::from_str_radix(k.trim_start_matches("0x"), 16).expect("hex"))
        .collect();
    let files = bf.active_files();
    println!("scanning {} files", files.len());
    for f in &files {
        let Ok(data) = decompress_entry(bf.read_data(f.index)) else {
            continue;
        };
        for r in walk_wow(&data) {
            if keys.contains(&r.key) {
                let len = r.data_range().len();
                if len > 56 {
                    println!(
                        "HIT key {:#x} in {} (len={} tag={} gro={:?})",
                        r.key,
                        f.name,
                        len,
                        r.tag,
                        r.gro_type
                    );
                }
            }
        }
    }
    println!("scan done");
    ExitCode::SUCCESS
}
