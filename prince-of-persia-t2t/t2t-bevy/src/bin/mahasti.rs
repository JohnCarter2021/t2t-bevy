//! Mahasti diagnosis + survey. Default mode dumps: her
//! bank's clip inventory (key, gro, frames), her bone
//! list, and the SG->B_KA name-map pairings (so a gown
//! mis-pairing shows itself). With MAH_CLIP=<hex> set,
//! renders that clip as a strip to out/mahasti_<key>_<mode>.png
//! with MAH_MAP=identity (default) or name.
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn load_sector(bf: &BigFile, substr: &str) -> Option<Vec<u8>> {
    let e = bf.active_files().into_iter().find(|e| e.name.contains(substr))?;
    decompress_entry(bf.read_data(e.index)).ok()
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let data = load_sector(&bf, "ACT_Enemy_Mahasti").expect("mahasti sector");
    let ka = Character::load(&data, 0x42002c7d, "B_KA").expect("ka rig");
    let recs = walk_wow(&data);

    if let Ok(hex) = std::env::var("MAH_CLIP") {
        let key = u32::from_str_radix(hex.trim_start_matches("0x"), 16).expect("hex key");
        let r = recs.iter().find(|r| r.key == key).expect("clip in bank");
        let clip = Clip::load(&data[r.data_range()]);
        let mode = std::env::var("MAH_MAP").unwrap_or_else(|_| "identity".into());
        let (map, tag) = if mode == "name" {
            let sg_data = load_sector(&bf, "ACT_Enemy_SandGarde").expect("sg");
            let sg = Character::load(&sg_data, 0x51001f50, "B_SG").expect("sg rig");
            (ChannelMap::from_bone_names(&sg, "B_SG", &ka, "B_KA"), "name")
        } else if mode == "shift" {
            // Her GAO list leads with the B_KA_Bip root, so
            // a clip authored in bone order addresses bones
            // 1..=35 from channels 0..=34.
            (ChannelMap::Header((0..35).map(|i| (i, i + 1)).collect()), "shift")
        } else {
            (ChannelMap::Identity, "identity")
        };
        let path = format!("out/mahasti_{key:08x}_{tag}.png");
        render_strip(&ka, &clip, &map, &path);
        println!("clip {key:#010x}: {} frames, {} rot channels -> {path}", clip.max_frame, clip.rot.len());
        return ExitCode::SUCCESS;
    }

    println!("== Mahasti bones ({} collected, prefix B_KA) ==", ka.bones.len());
    for (i, b) in ka.bones.iter().enumerate() {
        println!("  [{i:2}] {}", b.1);
    }
    println!("== bank clip containers ==");
    for r in &recs {
        if matches!(r.gro_type, Some(0x2002b) | Some(0x20025) | Some(0x2002c)) {
            let clip = Clip::load(&data[r.data_range()]);
            println!("  {:#010x} gro {:#x} frames {} rotch {}", r.key, r.gro_type.unwrap(), clip.max_frame, clip.rot.len());
        }
    }
    let sg_data = load_sector(&bf, "ACT_Enemy_SandGarde").expect("sg");
    let sg = Character::load(&sg_data, 0x51001f50, "B_SG").expect("sg rig");
    let map = ChannelMap::from_bone_names(&sg, "B_SG", &ka, "B_KA");
    println!("== SG -> B_KA name-map pairs ==");
    for (s, d) in map.pairs(ka.bones.len()) {
        println!("  SG[{s:2}] {:28} -> KA[{d:2}] {}", sg.bones[s].1, ka.bones[d].1);
    }
    ExitCode::SUCCESS
}
