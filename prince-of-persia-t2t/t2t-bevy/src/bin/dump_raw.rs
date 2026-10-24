//! dump_raw — write a BIG entry's RAW bytes (no LZO
//! decompression) to a file, for non-stream entries
//! like TimeLine.bin.
//! Usage: dump_raw <pop3.bf> <substr> <out-path>
use t2t::bigfile::BigFile;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage");
    let substr = std::env::args().nth(2).expect("usage");
    let out = std::env::args().nth(3).expect("usage");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&substr))
        .expect("entry not found");
    let data = bf.read_data(e.index);
    std::fs::write(&out, &data).expect("write");
    println!("{}: {} bytes -> {}", e.name, data.len(), out);
}
