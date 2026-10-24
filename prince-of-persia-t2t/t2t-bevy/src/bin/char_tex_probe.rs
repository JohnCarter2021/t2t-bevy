//! Debug: resolve a character GEO's per-element texture keys
//! through its owning GAO's material chain.
//! usage: char_tex_probe <bf> <sector-substr> <geo-key-hex>
//!   [world-sector-substr]
use std::collections::HashMap;
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::gao::parse_gao;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::material::texture_key_for;
use t2t::texture::{decode_texture, parse_texture};
use t2t::wow::walk_wow;

fn load_sector(bf: &BigFile, substr: &str) -> Option<Vec<u8>> {
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(substr))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let data = load_sector(&bf, &args[2]).expect("sector");
    let geo_key = u32::from_str_radix(args[3].trim_start_matches("0x"), 16).expect("hex");
    let records = walk_wow(&data);
    let mat_view: HashMap<u32, (u32, &[u8])> = records
        .iter()
        .filter_map(|r| r.gro_type.map(|g| (r.key, (g, &data[r.data_range()]))))
        .collect();
    let geo_rec = records.iter().find(|r| r.key == geo_key).expect("geo record");
    let geo = parse_geometry(&data[geo_rec.data_range()]).expect("geo parse");
    println!(
        "geo {:#x}: {} faces, {} elements, {} uvs",
        geo_key,
        geo.faces.len(),
        geo.elements.len(),
        geo.uvs.len() / 2
    );
    // Texture census of this sector.
    let mut tex_here: HashMap<u32, (u32, u32, u32)> = HashMap::new();
    for r in &records {
        if let Some(ti) = parse_texture(&data[r.data_range()]) {
            tex_here.insert(r.key, (ti.format, ti.width, ti.height));
        }
    }
    println!("texture records in this sector: {}", tex_here.len());
    // Owning GAOs: gro_key == geo, or a group record whose
    // payload mentions the geo key.
    let mut cands = Vec::new();
    for r in &records {
        if r.tag != ".gao" {
            continue;
        }
        let Some(gao) = parse_gao(&data[r.data_range()]) else {
            continue;
        };
        let direct = gao.gro_key == geo_key;
        let grouped = if !direct {
            let sub = &data[r.data_range()];
            let mut found = false;
            let mut o = 0;
            while o + 4 <= sub.len() {
                if u32::from_le_bytes([sub[o], sub[o + 1], sub[o + 2], sub[o + 3]]) == geo_key {
                    found = true;
                    break;
                }
                o += 1;
            }
            found
        } else {
            false
        };
        if (direct || grouped) && gao.grm_key != 0 {
            cands.push((gao.name.clone(), gao.gro_key, gao.grm_key, direct));
        }
    }
    println!("owning GAO candidates: {}", cands.len());
    for (name, gro, grm, direct) in &cands {
        println!("  GAO {name} gro={gro:#x} grm={grm:#x} direct={direct}");
    }
    // World-sector texture census (optional 4th arg).
    let mut tex_world: HashMap<u32, (u32, u32, u32)> = HashMap::new();
    if let Some(wsub) = args.get(4) {
        if let Some(wdata) = load_sector(&bf, wsub) {
            for r in walk_wow(&wdata) {
                if let Some(ti) = parse_texture(&wdata[r.data_range()]) {
                    tex_world.insert(r.key, (ti.format, ti.width, ti.height));
                }
            }
            println!("texture records in world sector {wsub}: {}", tex_world.len());
        }
    }
    for (name, _gro, grm, _) in &cands {
        println!("--- via GAO {name} grm={grm:#x} ---");
        let mut per_key: HashMap<Option<u32>, usize> = HashMap::new();
        for (ei, (_ntri, mat_id)) in geo.elements.iter().enumerate() {
            let tk = texture_key_for(*grm, *mat_id, &mat_view);
            *per_key.entry(tk).or_insert(0) += 1;
            let faces = geo.faces.iter().filter(|f| f[6] as usize == ei).count();
            let loc = match tk {
                Some(k) if tex_here.contains_key(&k) => "here",
                Some(k) if tex_world.contains_key(&k) => "world",
                Some(_) => "MISSING",
                None => "-",
            };
            let info = tk
                .and_then(|k| tex_here.get(&k).or_else(|| tex_world.get(&k)))
                .map(|(f, w, h)| format!("fmt={f} {w}x{h}"))
                .unwrap_or_default();
            println!("  elem {ei}: mat={mat_id} -> tex={tk:?} faces={faces} [{loc}] {info}");
        }
        println!("  distinct keys: {:?}", per_key);
    }
    // Decodability of the resolved keys in this sector.
    let mut seen = std::collections::HashSet::new();
    for (_n, _g, grm, _) in &cands {
        for (_ntri, mat_id) in &geo.elements {
            if let Some(k) = texture_key_for(*grm, *mat_id, &mat_view) {
                if seen.insert(k) {
                    // Last occurrence wins: sectors carry a 56-byte
                // stub before the full texture record.
                if let Some(r) = records.iter().rev().find(|r| r.key == k) {
                        if let Some(ti) = parse_texture(&data[r.data_range()]) {
                            let rgba = decode_texture(&data[r.data_range()], &ti);
                            println!(
                                "  decode {:#x}: fmt={} {}x{} -> {} bytes {}",
                                k,
                                ti.format,
                                ti.width,
                                ti.height,
                                rgba.len(),
                                if rgba.is_empty() { "EMPTY" } else { "ok" }
                            );
                        }
                    }
                }
            }
        }
    }
    ExitCode::SUCCESS
}
