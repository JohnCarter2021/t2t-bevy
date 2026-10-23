//! Locate a record key: which archive entries contain
//! it, and which skinned GEOs live in the same entries.
//! Usage: find_clip <bf> <key-hex>
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let want = u32::from_str_radix(args[2].trim_start_matches("0x"), 16).expect("key hex");
    for e in bf.active_files() {
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        let recs = walk_wow(&data);
        if recs.iter().any(|r| r.key == want) {
            let mut geos = Vec::new();
            for r in &recs {
                if r.gro_type == Some(1) {
                    if let Ok(g) = parse_geometry(&data[r.data_range()]) {
                        if g.skin.is_some() {
                            geos.push(format!("{:#010x}({}b)", r.key, g.skin.as_ref().unwrap().0));
                        }
                    }
                }
            }
            println!("{} contains {:#010x}; skinned GEOs: {:?}", e.name, want, geos);
        }
    }
    ExitCode::SUCCESS
}
