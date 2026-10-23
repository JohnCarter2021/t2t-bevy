//! find_char — scan every _wow_ in the archive for GAO names
//! containing a needle; report sector, GAO names, and any
//! skinned GEO those GAOs reference.
//!
//! Usage: find_char <pop3.bf> <needle>

use std::collections::BTreeMap;
use t2t::bigfile::BigFile;
use t2t::gao::parse_gao;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: find_char <pop3.bf> <needle>");
    let needle = std::env::args().nth(2).expect("usage: find_char <pop3.bf> <needle>");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let mut hits: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut geo_report: Vec<String> = Vec::new();
    for e in bf.active_files() {
        if !e.name.contains("_wow_") {
            continue;
        }
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        let records = walk_wow(&data);
        let by_key: std::collections::HashMap<u32, usize> = records
            .iter()
            .enumerate()
            .map(|(i, r)| (r.key, i))
            .collect();
        let mut names = Vec::new();
        for r in &records {
            let Some(g) = parse_gao(&data[r.data_range()]) else { continue };
            if !g.name.contains(&needle) {
                continue;
            }
            names.push(g.name.clone());
            let gro = g.gro_key;
            if gro != 0 {
                if let Some(&ri) = by_key.get(&gro) {
                    let rr = &records[ri];
                    if let Ok(geo) = parse_geometry(&data[rr.data_range()]) {
                        if geo.skin.is_some() || names.len() <= 2 {
                            geo_report.push(format!(
                                "{}: GAO {} -> GEO {:#010x} ({} pts, {} tris, skin: {})",
                                e.name,
                                g.name,
                                gro,
                                geo.nb_points(),
                                geo.nb_tris(),
                                geo.skin.is_some()
                            ));
                        }
                    }
                }
            }
        }
        if !names.is_empty() {
            hits.insert(e.name.clone(), names);
        }
    }
    for (sector, names) in &hits {
        println!("{sector}: {} GAOs", names.len());
        for n in names.iter().take(6) {
            println!("    {n}");
        }
        if names.len() > 6 {
            println!("    ... and {} more", names.len() - 6);
        }
    }
    println!("--- skinned/primary GEOs ---");
    for line in &geo_report {
        println!("{line}");
    }
}
