//! Debug: render one PAL8 texture with its prefix-matched
//! palette (BGRA entries) to PNG.
//! usage: pal_render <bf> <sector-substr> <key-hex> <out-png>
use std::collections::HashMap;
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::png::write_png_rgba;
use t2t::texture::{is_texture_entry, parse_texture};
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let entry = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&args[2]))
        .expect("entry");
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    let mut by_key: HashMap<u32, std::ops::Range<usize>> = HashMap::new();
    for r in walk_wow(&data) {
        by_key.insert(r.key, r.data_range());
    }
    let key = u32::from_str_radix(args[3].trim_start_matches("0x"), 16).expect("hex");
    let sub = &data[by_key[&key].clone()];
    let ti = parse_texture(sub).expect("not a texture");
    assert_eq!(ti.format, 1, "not PAL8");
    let prefix =
        u32::from_le_bytes([sub[52], sub[53], sub[54], sub[55]]);
    let pal = by_key.get(&prefix).map(|r| &data[r.clone()]).filter(|d| {
        d.len() == 1024 && !is_texture_entry(d)
    });
    println!("tex {:#x} {}x{} palette key {:#x} found={}", key, ti.width, ti.height, prefix, pal.is_some());
    let pix = &sub[ti.pix_start..];
    let need = ti.width as usize * ti.height as usize;
    let mut rgba = vec![0u8; need * 4];
    match pal {
        Some(p) => {
            for i in 0..need {
                let e = &p[pix[i] as usize * 4..pix[i] as usize * 4 + 4];
                rgba[4 * i] = e[2];
                rgba[4 * i + 1] = e[1];
                rgba[4 * i + 2] = e[0];
                rgba[4 * i + 3] = e[3];
            }
        }
        None => {
            for i in 0..need {
                rgba[4 * i] = pix[i];
                rgba[4 * i + 1] = pix[i];
                rgba[4 * i + 2] = pix[i];
                rgba[4 * i + 3] = 255;
            }
        }
    }
    // Stats: how many distinct palette indices are actually used.
    let mut used = [false; 256];
    for i in 0..need {
        used[pix[i] as usize] = true;
    }
    let used_n = used.iter().filter(|u| **u).count();
    let opaque = rgba.chunks_exact(4).filter(|px| px[3] == 255).count();
    println!("used indices={}/256 opaque pixels={}/{}", used_n, opaque, need);
    if let Some(p) = pal {
        // Alpha histogram over indices the texture actually uses.
        let mut buckets = [0u32; 8];
        for i in 0..256 {
            if used[i] {
                buckets[(p[i * 4 + 3] as usize) / 32] += 1;
            }
        }
        println!("used-index alpha buckets [0-31,32-63,...,224-255]: {:?}", buckets);
    }
    write_png_rgba(std::path::Path::new(&args[4]), ti.width, ti.height, &rgba)
        .expect("write png");
    println!("wrote {}", args[4]);
    // Alpha visualization: grey = alpha (white = opaque).
    let mut alpha_img = vec![0u8; need * 4];
    for i in 0..need {
        alpha_img[4 * i] = rgba[4 * i + 3];
        alpha_img[4 * i + 1] = rgba[4 * i + 3];
        alpha_img[4 * i + 2] = rgba[4 * i + 3];
        alpha_img[4 * i + 3] = 255;
    }
    let alpha_path = format!("{}.alpha.png", args[4]);
    write_png_rgba(std::path::Path::new(&alpha_path), ti.width, ti.height, &alpha_img)
        .expect("write alpha png");
    println!("wrote {}", alpha_path);
    ExitCode::SUCCESS
}
