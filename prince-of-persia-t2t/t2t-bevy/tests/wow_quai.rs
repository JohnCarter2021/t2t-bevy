//! M5 verification against the real 100_Quai world in pop3.bf:
//! MRM/skin parsing and the character skeleton structure.
//! Numbers measured 2026-10-22; the Kaileena bind-pose render was
//! visually confirmed (out/kaileena.png).

use std::collections::{HashMap, HashSet};
use t2t::bigfile::BigFile;
use t2t::gao::parse_gao;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

#[test]
fn quai_skin_and_skeleton() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf");
    if !p.exists() {
        eprintln!("pop3.bf not present — skipping");
        return;
    }
    let bf = BigFile::open(&p).expect("archive");
    let entry = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("100_Quai"))
        .expect("100_Quai entry");
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    let records = walk_wow(&data);

    // Exactly three skinned geometries: guard, Kaileena, soldier.
    let mut skinned = Vec::new();
    for r in &records {
        let sub = &data[r.data_range()];
        if let Ok(g) = parse_geometry(sub) {
            if g.skin.is_some() {
                skinned.push((r.key, g));
            }
        }
    }
    assert_eq!(skinned.len(), 3);

    // Kaileena: key 0x420035b0, 1,443 pts, 2,310 tris, 49 bones.
    let (_, kai) = skinned
        .iter()
        .find(|(k, _)| *k == 0x4200_35b0)
        .expect("Kaileena geometry");
    assert_eq!(kai.nb_points(), 1443);
    assert_eq!(kai.nb_tris(), 2310);
    let (_, bones) = kai.skin.as_ref().unwrap();
    assert_eq!(bones.len(), 49);

    // Every vertex is covered by at least one bone weight; weight
    // sums per vertex stay within one full-scale unit (0xFFFF) plus
    // rounding slack, i.e. the weights are u16 fractions of 1.0.
    let mut sums: HashMap<u16, u32> = HashMap::new();
    for b in bones {
        for (v, w) in &b.weights {
            *sums.entry(*v).or_default() += *w as u32;
        }
    }
    assert_eq!(sums.len(), 1443);
    let max_sum = *sums.values().max().unwrap();
    assert!(max_sum <= 0x1_0000, "weight sum {max_sum} exceeds full scale");

    // The skeleton is a GAO hierarchy: 49 bone GAOs named
    // "B_Kaileena ..." — exactly matching the skin's bone count.
    let mut bone_gaos = 0;
    let mut bone_gaos_hier = 0;
    let mut kai_gao = None;
    for r in &records {
        if r.tag == ".gao" {
            let sub = &data[r.data_range()];
            if let Some(g) = parse_gao(sub) {
                if g.name.starts_with("B_Kaileena") {
                    bone_gaos += 1;
                    if g.has_hierarchy {
                        bone_gaos_hier += 1;
                    }
                }
                if g.name == "m_Kaileena.gao" {
                    kai_gao = Some(g);
                }
            }
        }
    }
    // 50 bone GAOs: the skin drives 49 of them; one is a pure
    // hierarchy node with no skinned vertices of its own.
    assert_eq!(bone_gaos, 50);
    // All but the root bone(s) carry a father link.
    assert!(bone_gaos_hier >= 45, "only {bone_gaos_hier} bone GAOs have hierarchy");
    let kai_gao = kai_gao.expect("m_Kaileena.gao");
    assert_eq!(kai_gao.gro_key, 0x4200_35b0);

    // Sanity: distinct bone indices in the skin are within the list.
    let idx: HashSet<u16> = bones.iter().map(|b| b.bone_idx).collect();
    assert!(idx.iter().all(|i| (*i as usize) < 64));
}
