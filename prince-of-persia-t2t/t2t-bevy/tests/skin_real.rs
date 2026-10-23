//! Skinning pipeline verification against the real 100_Quai world
//! (2026-10-22): GAO skeleton composition x pelvis frame bridge x
//! GEO inverse-bind matrices reproduce Kaileena's bind pose.
//!
//! geo_global_b = pelvis_gao_global * compose(locals)_b
//! skin matrix  = geo_global_b * bind_matrix_b  == identity at bind
//! Bone order = wow record order of the B_Kaileena GAOs (== the GEO
//! skin list's bone_idx values). Known exception: the loincloth
//! chain and MOD BackCloth R04 (bone_idx 45..48) deviate — a cloth
//! indexing quirk documented in docs/FORMATS.md.

use std::collections::HashMap;
use t2t::bigfile::BigFile;
use t2t::gao::{mat_mul, parse_gao, transform_point, IDENTITY};
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

#[test]
fn kaileena_bind_skinning_is_identity() {
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

    // Skeleton in record order.
    let mut bones: Vec<(u32, u32, Option<[f32; 16]>, Option<[f32; 16]>)> = Vec::new();
    let mut by_key: HashMap<u32, usize> = HashMap::new();
    for r in &records {
        if r.tag == ".gao" {
            if let Some(g) = parse_gao(&data[r.data_range()]) {
                if g.name.starts_with("B_Kaileena") {
                    by_key.insert(r.key, bones.len());
                    bones.push((r.key, g.father_key, g.local_matrix, g.global_matrix));
                }
            }
        }
    }
    assert_eq!(bones.len(), 50);
    let root_global = bones[0].3.expect("pelvis global");

    fn compose(
        i: usize,
        bones: &[(u32, u32, Option<[f32; 16]>, Option<[f32; 16]>)],
        by_key: &HashMap<u32, usize>,
        cache: &mut HashMap<usize, [f32; 16]>,
    ) -> [f32; 16] {
        if let Some(m) = cache.get(&i) {
            return *m;
        }
        let local = bones[i].2.unwrap_or(IDENTITY);
        let m = if bones[i].1 == 0 {
            local
        } else if let Some(&fi) = by_key.get(&bones[i].1) {
            mat_mul(&compose(fi, bones, by_key, cache), &local)
        } else {
            local
        };
        cache.insert(i, m);
        m
    }
    let mut cache = HashMap::new();
    let globals: Vec<[f32; 16]> = (0..bones.len())
        .map(|i| compose(i, &bones, &by_key, &mut cache))
        .collect();

    // Kaileena GEO + skin.
    let geo_rec = records.iter().find(|r| r.key == 0x4200_35b0).expect("geo");
    let geo = parse_geometry(&data[geo_rec.data_range()]).expect("geo");
    let (_, skin) = geo.skin.as_ref().expect("skin");
    assert_eq!(skin.len(), 49);

    // Bind skin matrices are identity (3x4 part) for all bones
    // except the documented cloth set 45..48.
    for b in skin {
        if (45..=48).contains(&b.bone_idx) {
            continue;
        }
        let geo_g = mat_mul(&root_global, &globals[b.bone_idx as usize]);
        let s = mat_mul(&geo_g, &b.bind_matrix);
        for (i, v) in s.iter().enumerate() {
            if i == 15 {
                continue; // Jade matrices carry ~0 in m[15]
            }
            let want = if i % 5 == 0 { 1.0 } else { 0.0 };
            assert!(
                (v - want).abs() < 1e-4,
                "bone {} S[{i}] = {v}, want {want}",
                b.bone_idx
            );
        }
    }

    // Skinning the bind pose with sum-normalized weights reproduces
    // the raw geometry AABB exactly. (Raw MRM weights sum to
    // 0.25/0.5/0.75 per vertex, never 1.0.)
    let mut vweights: Vec<Vec<(usize, f32)>> = vec![Vec::new(); geo.nb_points()];
    for (pos, b) in skin.iter().enumerate() {
        for &(v, w) in &b.weights {
            vweights[v as usize].push((pos, w as f32 / 65535.0));
        }
    }
    let smats: Vec<[f32; 16]> = skin
        .iter()
        .map(|b| mat_mul(&mat_mul(&root_global, &globals[b.bone_idx as usize]), &b.bind_matrix))
        .collect();
    let (mut mn, mut mx) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for v in 0..geo.nb_points() {
        let p = [geo.vertices[3 * v], geo.vertices[3 * v + 1], geo.vertices[3 * v + 2]];
        let mut acc = [0f32; 3];
        let mut ws = 0f32;
        for &(pos, w) in &vweights[v] {
            let tp = transform_point(&smats[pos], p);
            acc[0] += w * tp[0];
            acc[1] += w * tp[1];
            acc[2] += w * tp[2];
            ws += w;
        }
        let q = if ws > 0.0 {
            [acc[0] / ws, acc[1] / ws, acc[2] / ws]
        } else {
            p
        };
        for i in 0..3 {
            mn[i] = mn[i].min(q[i]);
            mx[i] = mx[i].max(q[i]);
        }
    }
    let (gmn, gmx) = geo.aabb();
    for i in 0..3 {
        assert!((mn[i] - gmn[i]).abs() < 1e-3, "aabb min[{i}]");
        assert!((mx[i] - gmx[i]).abs() < 1e-3, "aabb max[{i}]");
    }
}
