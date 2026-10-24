//! Debug: census world textures for a sector: formats, sizes,
//! face coverage. usage: tex_census <bf> <sector-substr>
use std::collections::HashMap;
use std::process::ExitCode;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::texture::decode_texture;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let entry = bf
        .active_files()
        .iter()
        .find(|f| f.name.contains(&args[2]))
        .expect("entry")
        .clone();
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let s = &world.stats;
    println!(
        "placed={} elements={} with_tex_key={} resolved={} stub={} missing={}",
        world.placed.len(),
        s.elements,
        s.elements_with_texture_key,
        s.texture_keys_resolved_to_pixels,
        s.texture_keys_stub_only,
        s.texture_keys_missing
    );
    // Face coverage + format histogram over USED keys.
    let mut used: HashMap<u32, usize> = HashMap::new();
    let mut untex_faces = 0usize;
    let mut tex_faces = 0usize;
    for p in &world.placed {
        for (ei, f) in p.geo.faces.iter().enumerate() {
            let _ = ei;
            match p.element_textures.get(f[6] as usize).copied().flatten() {
                Some(k) => {
                    *used.entry(k).or_insert(0) += 1;
                    tex_faces += 1;
                }
                None => untex_faces += 1,
            }
        }
    }
    println!(
        "faces: textured={} untextured={} distinct_keys_used={}",
        tex_faces,
        untex_faces,
        used.len()
    );
    let mut fmt_hist: HashMap<u32, (usize, usize)> = HashMap::new();
    let mut total_rgba = 0usize;
    let mut undecodable = 0usize;
    for (k, _) in &used {
        if let Some((ti, range)) = world.textures.get(k) {
            let rgba = decode_texture(&data[range.clone()], ti);
            if rgba.is_empty() {
                undecodable += 1;
            } else {
                total_rgba += rgba.len();
            }
            let e = fmt_hist.entry(ti.format).or_insert((0, 0));
            e.0 += 1;
            e.1 += ti.width as usize * ti.height as usize;
        }
    }
    println!("format histogram (keys, pixels): {fmt_hist:?}");
    println!("total decoded RGBA bytes: {total_rgba} ({:.1} MiB), undecodable keys: {undecodable}", total_rgba as f64 / 1048576.0);
    // Largest textures.
    let mut sizes: Vec<(u32, u32, u32, u32)> = used
        .keys()
        .filter_map(|k| {
            world
                .textures
                .get(k)
                .map(|(ti, _)| (*k, ti.format, ti.width, ti.height))
        })
        .collect();
    sizes.sort_by_key(|&(_, _, w, h)| std::cmp::Reverse(w * h));
    println!("largest 8: {sizes:?}", sizes = &sizes[..sizes.len().min(8)]);
    ExitCode::SUCCESS
}
