//! Census: resolve EVERY ACT_Enemy bank from the name
//! map. For each bank: find its rig (skinned GEO + Bip
//! GAO prefix), build the bone-name channel map from
//! the Sand Garde's bone order, count paired channels,
//! and render the shared pool's lunging cycle on the rig
//! through the map to out/resolve/<bank>.png.
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::gao::parse_gao;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn load_sector(bf: &BigFile, substr: &str) -> Option<Vec<u8>> {
    let e = bf.active_files().into_iter().find(|e| e.name.contains(substr))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

fn aabb(v: &[f32]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for p in v.chunks_exact(3) {
        for i in 0..3 {
            mn[i] = mn[i].min(p[i]);
            mx[i] = mx[i].max(p[i]);
        }
    }
    (mn, mx)
}

fn render_strip(ch: &Character, clip: &Clip, map: &ChannelMap, path: &str) {
    let frames = [0, clip.max_frame / 3, 2 * clip.max_frame / 3, clip.max_frame];
    let (w, h) = (360usize, 300usize);
    let mut fb = vec![vec![[24u8, 24, 28]; w * h]; 4];
    let mut zb = vec![vec![f32::NEG_INFINITY; w * h]; 4];
    for (fi, &fr) in frames.iter().enumerate() {
        let verts = posed_verts(ch, clip, map, fr);
        for f in &ch.faces {
            let pts: Vec<[f32; 3]> = (0..3)
                .map(|k| {
                    let o = f[k] as usize * 3;
                    [verts[o], verts[o + 1], verts[o + 2]]
                })
                .collect();
            let proj = |p: [f32; 3]| -> [f32; 3] {
                [(p[0] / 2.4 + 0.5) * w as f32, (1.0 - (p[2] + 0.1) / 2.4) * h as f32, p[1]]
            };
            let a = proj(pts[0]);
            let b = proj(pts[1]);
            let c = proj(pts[2]);
            let e1 = [b[0] - a[0], b[1] - a[1]];
            let e2 = [c[0] - a[0], c[1] - a[1]];
            let nz = (e1[0] * e2[1] - e1[1] * e2[0]).abs();
            let shade = (140.0 + nz * 2.0).min(235.0) as u8;
            let (mn_x, mx_x) = (a[0].min(b[0]).min(c[0]).max(0.0), a[0].max(b[0]).max(c[0]).min(w as f32 - 1.0));
            let (mn_y, mx_y) = (a[1].min(b[1]).min(c[1]).max(0.0), a[1].max(b[1]).max(c[1]).min(h as f32 - 1.0));
            let denom = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if denom.abs() < 1e-9 {
                continue;
            }
            for y in mn_y as usize..=mx_y as usize {
                for x in mn_x as usize..=mx_x as usize {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / denom;
                    let w1 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / denom;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let d = w0 * a[2] + w1 * b[2] + w2 * c[2];
                    let idx = y * w + x;
                    if d > zb[fi][idx] {
                        zb[fi][idx] = d;
                        fb[fi][idx] = [shade, shade, shade];
                    }
                }
            }
        }
    }
    let mut out = vec![[16u8, 16, 20]; w * 4 * h];
    for fi in 0..4 {
        for y in 0..h {
            for x in 0..w {
                out[y * w * 4 + fi * w + x] = fb[fi][y * w + x];
            }
        }
    }
    let rgba: Vec<u8> = out.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
    t2t::png::write_png_rgba(std::path::Path::new(path), (w * 4) as u32, h as u32, &rgba).expect("write png");
}

/// Prefix candidates from a pelvis GAO name:
/// "B_SG_Bip Pelvis.gao" -> ["B_SG", "B_SG_Bip"].
fn prefix_candidates(pelvis_name: &str) -> Option<Vec<String>> {
    let idx = pelvis_name.find("Pelvis")?;
    let base = pelvis_name[..idx].trim_end_matches(['_', ' ']).to_string();
    let mut out = vec![];
    if let Some(stripped) = base.strip_suffix("Bip") {
        out.push(stripped.trim_end_matches(['_', ' ']).to_string());
    }
    out.push(base);
    Some(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let sg_data = load_sector(&bf, "ACT_Enemy_SandGarde").expect("sg sector");
    let sg = Character::load(&sg_data, 0x51001f50, "B_SG").expect("sg rig");
    let sg_recs = walk_wow(&sg_data);
    let cycle_rec = sg_recs.iter().find(|r| r.key == 0x6c0019e5).expect("pool cycle");
    let cycle = Clip::load(&sg_data[cycle_rec.data_range()]);

    std::fs::create_dir_all("out/resolve").expect("mkdir");
    let mut names: Vec<String> = bf
        .active_files()
        .into_iter()
        .filter(|f| f.name.contains("ACT_Enemy"))
        .map(|f| f.name.clone())
        .collect();
    names.sort();
    names.dedup();
    println!("{} ACT_Enemy entries", names.len());
    for name in &names {
        let short = name
            .trim_start_matches("ROOT/Bin/")
            .trim_end_matches(".bin")
            .split("_wow")
            .next()
            .unwrap_or(name);
        let data = load_sector(&bf, name).unwrap_or_default();
        let recs = walk_wow(&data);
        // Pelvis GAOs -> prefix candidates.
        let mut cands: Vec<String> = Vec::new();
        for r in &recs {
            if let Some(g) = parse_gao(&data[r.data_range()]) {
                if g.name.contains("Pelvis") {
                    if let Some(mut cs) = prefix_candidates(&g.name) {
                        cands.append(&mut cs);
                    }
                }
            }
        }
        cands.sort();
        cands.dedup();
        // Skinned GEOs.
        let geos: Vec<u32> = recs
            .iter()
            .filter(|r| r.gro_type == Some(1))
            .filter(|r| {
                parse_geometry(&data[r.data_range()])
                    .map(|g| g.skin.is_some())
                    .unwrap_or(false)
            })
            .map(|r| r.key)
            .collect();
        let own_clips = recs
            .iter()
            .filter(|r| matches!(r.gro_type, Some(0x2002b) | Some(0x20025)))
            .count();
        let mut resolved = false;
        for geo in &geos {
            for cand in &cands {
                if let Some(ch) = Character::load(&data, *geo, cand) {
                    if ch.bones.len() < 15 {
                        continue;
                    }
                    // The GEO's skin indices must address the
                    // collected bones (a wrong prefix pairing
                    // loads but cannot pose).
                    if !ch.skin.iter().all(|(bi, _)| (*bi as usize) < ch.bones.len()) {
                        continue;
                    }
                    let map = ChannelMap::from_bone_names(&sg, "B_SG", &ch, cand);
                    let pairs = map.pairs(ch.bones.len()).len();
                    let v = posed_verts(&ch, &cycle, &map, 7);
                    let (mn, mx) = aabb(&v);
                    let path = format!(
                        "out/resolve/{}_{geo:08x}_{}.png",
                        short.replace("ACT_Enemy_", ""),
                        cand.replace(' ', "_")
                    );
                    render_strip(&ch, &cycle, &map, &path);
                    println!(
                        "{short}: geo {geo:#010x} prefix {cand:?} bones {} pairs {pairs}/46 own-clips {own_clips} mapAABB h {:.2} w {:.2} -> {path}",
                        ch.bones.len(),
                        mx[2] - mn[2],
                        ((mx[0] - mn[0]).powi(2) + (mx[1] - mn[1]).powi(2)).sqrt()
                    );
                    resolved = true;
                }
            }
        }
        if !resolved {
            println!("{short}: NO RIG (geos {} cands {:?} own-clips {own_clips})", geos.len(), cands);
        }
    }
    ExitCode::SUCCESS
}
