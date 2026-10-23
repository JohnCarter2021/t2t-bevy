//! dump_sector — write a sector's decompressed wow stream
//! to a file for offline analysis (strings, hex).
//!
//! Usage: dump_sector <pop3.bf> <sector-substr> <out-path>

use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: dump_sector <bf> <substr> <out>");
    let substr = std::env::args().nth(2).expect("usage");
    let out = std::env::args().nth(3).expect("usage");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&substr))
        .expect("sector not found");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    std::fs::write(&out, &data).expect("write");
    println!("{}: {} bytes -> {}", e.name, data.len(), out);
}
