//! scan_skins — find every skinned GEO in the archive, the
//! GAOs that reference it, and how many sectors embed it.
//!
//! Usage: scan_skins <pop3.bf>

use std::collections::BTreeMap;
use t2t::bigfile::BigFile;
use t2t::gao::parse_gao;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

#[derive(Default)]
struct Agg {
    sectors: Vec<String>,
    gaos: Vec<String>,
    pts: usize,
    tris: usize,
    bones: usize,
}

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: scan_skins <pop3.bf>");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let mut agg: BTreeMap<u32, Agg> = BTreeMap::new();
    for e in bf.active_files() {
        if !e.name.contains("_wow_") {
            continue;
        }
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        let records = walk_wow(&data);
        // key -> referencing GAO names in this sector
        let mut refs: std::collections::HashMap<u32, Vec<String>> = Default::default();
        for r in &records {
            if let Some(g) = parse_gao(&data[r.data_range()]) {
                if g.gro_key != 0 {
                    refs.entry(g.gro_key).or_default().push(g.name.clone());
                }
            }
        }
        for r in &records {
            let Ok(geo) = parse_geometry(&data[r.data_range()]) else { continue };
            let Some((_, bones)) = &geo.skin else { continue };
            let a = agg.entry(r.key).or_default();
            a.pts = geo.nb_points();
            a.tris = geo.nb_tris();
            a.bones = bones.len();
            a.sectors.push(e.name.clone());
            if let Some(names) = refs.get(&r.key) {
                for n in names {
                    if !a.gaos.contains(n) {
                        a.gaos.push(n.clone());
                    }
                }
            }
        }
    }
    let mut rows: Vec<_> = agg.iter().collect();
    rows.sort_by_key(|(_, a)| std::cmp::Reverse(a.sectors.len()));
    for (key, a) in rows {
        println!(
            "GEO {:#010x}: {} pts, {} tris, {} skin bones, in {} sectors; GAOs: {}",
            key,
            a.pts,
            a.tris,
            a.bones,
            a.sectors.len(),
            a.gaos.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
        );
        println!("    first sectors: {}", a.sectors.iter().take(4).cloned().collect::<Vec<_>>().join(", "));
    }
}
