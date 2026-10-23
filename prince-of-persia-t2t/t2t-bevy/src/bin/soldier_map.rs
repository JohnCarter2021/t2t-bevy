//! Test: can the shared enemy pool play on the Babylonian
//! Soldier via a NAME-based channel map (SG bone order ->
//! soldier bone order, matched by bone-name suffix)?
//! usage: soldier_map <bf> <clip-hex>
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn load_sector(bf: &BigFile, substr: &str) -> Option<Vec<u8>> {
    let e = bf.active_files().into_iter().find(|e| e.name.contains(substr))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

fn suffix(name: &str, prefix: &str) -> String {
    // "B_SG_Bip L ForeArm.gao" -> "l forearm";
    // "B_BabylonSoldier L Forearm.gao" -> "l forearm".
    let s = name.strip_prefix(prefix).unwrap_or(name);
    let s = s.trim_start_matches(['_', ' ']);
    let s = s.strip_prefix("Bip").unwrap_or(s);
    let s = s.trim_start_matches(['_', ' ']);
    let s = s.strip_suffix(".gao").unwrap_or(s);
    s.trim().to_lowercase()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let want = u32::from_str_radix(args[2].trim_start_matches("0x"), 16).unwrap();

    let sg_data = load_sector(&bf, "ACT_Enemy_SandGarde").expect("sg sector");
    let sg = Character::load(&sg_data, 0x51001f50, "B_SG").expect("sg rig");
    let so_data = load_sector(&bf, "100_Quai").expect("quai sector");
    let so = Character::load(&so_data, 0x42003196, "B_BabylonSoldier").expect("soldier rig");

    println!("SG bones ({}):", sg.bones.len());
    for (i, b) in sg.bones.iter().enumerate() {
        println!("  [{i}] {}", b.1);
    }
    println!("Soldier bones ({}):", so.bones.len());
    for (i, b) in so.bones.iter().enumerate() {
        println!("  [{i}] {}", b.1);
    }

    // Name-based pairs: channel = SG index, bone = soldier
    // index with the same suffix.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (ci, b) in sg.bones.iter().enumerate() {
        let suf = suffix(&b.1, "B_SG");
        if suf.is_empty() {
            continue;
        }
        if let Some((bi, _)) = so
            .bones
            .iter()
            .enumerate()
            .find(|(_, sb)| suffix(&sb.1, "B_BabylonSoldier") == suf)
        {
            pairs.push((ci, bi));
        }
    }
    println!("name-mapped pairs: {} of {} SG channels", pairs.len(), sg.bones.len());
    let map = ChannelMap::Header(pairs);

    let recs = walk_wow(&sg_data);
    let r = recs.iter().find(|r| r.key == want).expect("clip");
    let clip = Clip::load(&sg_data[r.data_range()]);
    let frames = [0, clip.max_frame / 3, 2 * clip.max_frame / 3, clip.max_frame];
    let (w, h) = (360usize, 300usize);
    let mut fb = vec![vec![[24u8, 24, 28]; w * h]; 4];
    let mut zb = vec![vec![f32::NEG_INFINITY; w * h]; 4];
    for (fi, &fr) in frames.iter().enumerate() {
        let verts = posed_verts(&so, &clip, &map, fr);
        for f in &so.faces {
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
    let path = format!("out/soldier_map_{:08x}.png", want);
    t2t::png::write_png_rgba(std::path::Path::new(&path), (w * 4) as u32, h as u32, &rgba).expect("write png");
    println!("wrote {path}");
    ExitCode::SUCCESS
}
