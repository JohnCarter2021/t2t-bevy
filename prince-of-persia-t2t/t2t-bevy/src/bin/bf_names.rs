//! bf_names — print every active BIG entry name, one per
//! line, for offline grepping (sector/level/chapter hunts).
//! Usage: bf_names <pop3.bf>
use t2t::bigfile::BigFile;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage");
    let bf = BigFile::open(&bf_path).expect("open bf");
    for e in bf.active_files() {
        println!("{}", e.name);
    }
}
