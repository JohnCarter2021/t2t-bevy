//! Render an 8-frame strip of one clip on one rig.
//! Usage: strip <bf> <clip-sector> <clip-hex> <rig-sector>
//!        <geo-hex> <prefix> <out.png>
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::png::write_png_rgba;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn sector(bf: &BigFile, substr: &str) -> Vec<u8> {
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(substr))
        .unwrap_or_else(|| panic!("sector {substr}"));
    decompress_entry(bf.read_data(e.index)).expect("decompress")
}

fn main() -> ExitCode {
    let a: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&a[1]).expect("archive");
    let clip_key = u32::from_str_radix(a[3].trim_start_matches("0x"), 16).expect("clip hex");
    let geo_key = u32::from_str_radix(a[5].trim_start_matches("0x"), 16).expect("geo hex");
    let cdata = sector(&bf, &a[2]);
    let crec = walk_wow(&cdata)
        .into_iter()
        .find(|r| r.key == clip_key)
        .expect("clip record");
    let clip = Clip::load(&cdata[crec.data_range()]);
    let rdata = sector(&bf, &a[4]);
    let ch = Character::load(&rdata, geo_key, &a[6]).expect("rig");
    let n = 8usize;
    let (w, h) = (220usize, 300usize);
    let mut fb = vec![vec![[24u8, 24, 28]; w * h]; n];
    let mut zb = vec![vec![f32::NEG_INFINITY; w * h]; n];
    for fi in 0..n {
        let fr = (clip.max_frame as usize * fi / (n - 1)) as u32;
        let verts = posed_verts(&ch, &clip, &ChannelMap::Identity, fr);
        for f in &ch.faces {
            let pts: Vec<[f32; 3]> = (0..3)
                .map(|k| {
                    let o = f[k] as usize * 3;
                    [verts[o], verts[o + 1], verts[o + 2]]
                })
                .collect();
            let proj = |p: [f32; 3]| -> [f32; 3] {
                [
                    (p[0] / 2.4 + 0.5) * w as f32,
                    (1.0 - (p[2] + 0.1) / 2.4) * h as f32,
                    p[1],
                ]
            };
            let (pa, pb, pc) = (proj(pts[0]), proj(pts[1]), proj(pts[2]));
            let e1 = [pb[0] - pa[0], pb[1] - pa[1]];
            let e2 = [pc[0] - pa[0], pc[1] - pa[1]];
            let nz = (e1[0] * e2[1] - e1[1] * e2[0]).abs();
            let shade = (140.0 + nz * 2.0).min(235.0) as u8;
            let (mn_x, mx_x) = (
                pa[0].min(pb[0]).min(pc[0]).max(0.0),
                pa[0].max(pb[0]).max(pc[0]).min(w as f32 - 1.0),
            );
            let (mn_y, mx_y) = (
                pa[1].min(pb[1]).min(pc[1]).max(0.0),
                pa[1].max(pb[1]).max(pc[1]).min(h as f32 - 1.0),
            );
            let denom = (pb[1] - pc[1]) * (pa[0] - pc[0]) + (pc[0] - pb[0]) * (pa[1] - pc[1]);
            if denom.abs() < 1e-9 {
                continue;
            }
            for y in mn_y as usize..=mx_y as usize {
                for x in mn_x as usize..=mx_x as usize {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((pb[1] - pc[1]) * (px - pc[0]) + (pc[0] - pb[0]) * (py - pc[1])) / denom;
                    let w1 = ((pc[1] - pa[1]) * (px - pc[0]) + (pa[0] - pc[0]) * (py - pc[1])) / denom;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let d = w0 * pa[2] + w1 * pb[2] + w2 * pc[2];
                    let idx = y * w + x;
                    if d > zb[fi][idx] {
                        zb[fi][idx] = d;
                        fb[fi][idx] = [shade, shade, shade];
                    }
                }
            }
        }
    }
    let mut img = vec![[24u8, 24, 28]; w * n * h];
    for fi in 0..n {
        for y in 0..h {
            for x in 0..w {
                img[y * (w * n) + fi * w + x] = fb[fi][y * w + x];
            }
        }
    }
    let flat: Vec<u8> = img.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
    write_png_rgba(std::path::Path::new(&a[7]), (w * n) as u32, h as u32, &flat).expect("png");
    println!(
        "wrote {} (clip {:#x}, {} frames)",
        a[7], clip_key, clip.max_frame
    );
    ExitCode::SUCCESS
}
