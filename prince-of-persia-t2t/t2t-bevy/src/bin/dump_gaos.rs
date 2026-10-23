//! dump_gaos — print every GAO name + key in one sector.
//! Usage: dump_gaos <pop3.bf> <sector-substring>

use t2t::bigfile::BigFile;
use t2t::gao::parse_gao;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: dump_gaos <pop3.bf> <substr>");
    let substr = std::env::args().nth(2).expect("usage: dump_gaos <pop3.bf> <substr>");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&substr))
        .expect("sector not found");
    println!("sector: {}", e.name);
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    for r in walk_wow(&data) {
        if let Some(g) = parse_gao(&data[r.data_range()]) {
            println!("{:#010x}  {}  (gro {:#010x})", r.key, g.name, g.gro_key);
        }
    }
}
