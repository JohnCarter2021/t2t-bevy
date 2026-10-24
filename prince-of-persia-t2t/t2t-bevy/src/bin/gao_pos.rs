//! gao_pos — print a GAO's matrix translation (Jade coords)
//! for one named object in a sector.
//! Usage: gao_pos <bf> <sector-substr> <gao-name-substr>
use t2t::bigfile::BigFile;
use t2t::gao::parse_gao;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage");
    let substr = std::env::args().nth(2).expect("usage");
    let want = std::env::args().nth(3).expect("usage");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&substr))
        .expect("sector not found");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    for r in walk_wow(&data) {
        if r.tag != ".gao" {
            continue;
        }
        let range = r.data_range();
        let g = match parse_gao(&data[range]) {
            Some(g) => g,
            None => continue,
        };
        if g.name.contains(&want) {
            match g.global_matrix {
                Some(m) => println!(
                    "{} key=0x{:08x} pos=[{:.3},{:.3},{:.3}]",
                    g.name, r.key, m[12], m[13], m[14]
                ),
                None => println!("{} key=0x{:08x} pos=NONE", g.name, r.key),
            }
        }
    }
}
