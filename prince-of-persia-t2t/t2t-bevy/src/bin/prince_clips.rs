//! prince_clips — cross-sector clip survey: load a character
//! from one sector, then score every clip record of another
//! sector by duration and pose change (locomotion hunting).
//!
//! Usage: prince_clips <pop3.bf> <rig-sector> <geo-hex> <prefix> <clip-sector>

use t2t::bigfile::BigFile;
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

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage");
    let rig_sector = std::env::args().nth(2).expect("usage");
    let geo_key = u32::from_str_radix(std::env::args().nth(3).expect("usage").trim_start_matches("0x"), 16).expect("hex");
    let prefix = std::env::args().nth(4).expect("usage");
    let clip_sector = std::env::args().nth(5).expect("usage");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let rig_data = load_sector(&bf, &rig_sector).expect("rig sector");
    let ch = Character::load(&rig_data, geo_key, &prefix).expect("character");
    let clip_data = load_sector(&bf, &clip_sector).expect("clip sector");
    let records = walk_wow(&clip_data);
    // Render mode: PRINCE_RENDER=<clip-hex> writes a 4-frame
    // strip PNG (front view) to out/prince_strip_<key>.png.
    if let Ok(want) = std::env::var("PRINCE_RENDER") {
        let want = u32::from_str_radix(want.trim_start_matches("0x"), 16).unwrap();
        for r in &records {
            if r.key != want {
                continue;
            }
            let clip = Clip::load(&clip_data[r.data_range()]);
            let frames = [0, clip.max_frame / 3, 2 * clip.max_frame / 3, clip.max_frame];
            let (w, h) = (360usize, 300usize);
            let mut fb = vec![vec![[24u8, 24, 28]; w * h]; 4];
            let mut zb = vec![vec![f32::NEG_INFINITY; w * h]; 4];
            for (fi, &fr) in frames.iter().enumerate() {
                let verts = posed_verts(&ch, &clip, &ChannelMap::Identity, fr);
                for f in &ch.faces {
                    let pts: Vec<[f32; 3]> = (0..3)
                        .map(|k| {
                            let o = f[k] as usize * 3;
                            [verts[o], verts[o + 1], verts[o + 2]]
                        })
                        .collect();
                    // Orthographic front view: x right, z up,
                    // depth = y. Character ~2 m tall.
                    let proj = |p: [f32; 3]| -> [f32; 3] {
                        [
                            (p[0] / 2.4 + 0.5) * w as f32,
                            (1.0 - (p[2] + 0.1) / 2.4) * h as f32,
                            p[1],
                        ]
                    };
                    let a = proj(pts[0]);
                    let b = proj(pts[1]);
                    let c = proj(pts[2]);
                    // Flat shading from the 2D edge normal.
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
            // Compose the strip side by side.
            let mut out = vec![[16u8, 16, 20]; w * 4 * h];
            for fi in 0..4 {
                for y in 0..h {
                    for x in 0..w {
                        out[y * w * 4 + fi * w + x] = fb[fi][y * w + x];
                    }
                }
            }
            let rgba: Vec<u8> = out.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
            let path = format!("out/prince_strip_{:08x}.png", r.key);
            t2t::png::write_png_rgba(std::path::Path::new(&path), (w * 4) as u32, h as u32, &rgba).expect("write png");
            println!("wrote {path} (frames {frames:?} of {})", clip.max_frame + 1);
        }
        return;
    }
    // Trace mode: PRINCE_TRACE=<clip-hex> prints L/R foot
    // heights over the clip (run cycles alternate feet).
    if let Ok(want) = std::env::var("PRINCE_TRACE") {
        let want = u32::from_str_radix(want.trim_start_matches("0x"), 16).unwrap();
        for r in &records {
            if r.key != want {
                continue;
            }
            let clip = Clip::load(&clip_data[r.data_range()]);
            for frame in (0..=clip.max_frame).step_by(2) {
                let locals = t2t::skin::pose_locals(&ch, &clip, &ChannelMap::Identity, frame);
                let globals = ch.compose_globals(&locals);
                // Jade matrices are column-major: translation
                // at [12],[13],[14]; z = [14]. But compose_globals
                // works in the skeleton frame, whose up axis is
                // the frame-bridge's business — print all three.
                let lt = [globals[28][12], globals[28][13], globals[28][14]];
                let rt = [globals[32][12], globals[32][13], globals[32][14]];
                let pt = [globals[0][12], globals[0][13], globals[0][14]];
                println!("f{frame:3}: pelvis {pt:?} L foot {lt:?} R foot {rt:?}");
            }
        }
        return;
    }
    for r in &records {
        if !matches!(r.gro_type, Some(0x2002b) | Some(0x20025)) {
            continue;
        }
        let clip = Clip::load(&clip_data[r.data_range()]);
        if clip.max_frame == 0 {
            continue;
        }
        let f = clip.max_frame;
        let v0 = posed_verts(&ch, &clip, &ChannelMap::Identity, 0);
        let v1 = posed_verts(&ch, &clip, &ChannelMap::Identity, f / 2);
        let v2 = posed_verts(&ch, &clip, &ChannelMap::Identity, f);
        let (mn0, mx0) = aabb(&v0);
        let (mn1, _) = aabb(&v1);
        let (mn2, _) = aabb(&v2);
        // Silhouette travel: min-corner drift start->mid->end.
        let d_mid = ((mn1[0] - mn0[0]).powi(2) + (mn1[1] - mn0[1]).powi(2)).sqrt();
        let d_end = ((mn2[0] - mn0[0]).powi(2) + (mn2[1] - mn0[1]).powi(2)).sqrt();
        let h0 = mx0[2] - mn0[2];
        // Pose delta: RMS vertex displacement start -> mid.
        let mut acc = 0f32;
        for i in 0..v0.len() {
            acc += (v1[i] - v0[i]).powi(2);
        }
        let rms = (acc / v0.len() as f32).sqrt();
        println!(
            "clip {:#010x}: {} frames, {} rot chans, height {:.2}, drift mid {:.2} end {:.2}, poseRMS {:.3}",
            r.key,
            f + 1,
            clip.rot.len(),
            h0,
            d_mid,
            d_end,
            rms
        );
    }
}
