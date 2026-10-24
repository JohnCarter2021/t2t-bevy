//! Debug: verify the PAL8 palette link — the 4-byte prefix at
//! offset 52 of a format-1 texture record, read LE, is the key
//! of its 1024-byte palette record (Jade-Toolkit palette_for_texture).
//! usage: pal_link <bf> <sector-substr>
use std::collections::HashMap;
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
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
    // Last occurrence wins (stub-then-full rule).
    let mut by_key: HashMap<u32, std::ops::Range<usize>> = HashMap::new();
    for r in walk_wow(&data) {
        by_key.insert(r.key, r.data_range());
    }
    let mut pal_keys: Vec<u32> = Vec::new();
    for (k, range) in &by_key {
        let d = &data[range.clone()];
        if d.len() == 1024 && !is_texture_entry(d) {
            let opaque = d.chunks_exact(4).filter(|px| px[3] == 255).count();
            println!("palette candidate key {:#x} alpha255={}/256", k, opaque);
            pal_keys.push(*k);
        }
    }
    println!("--- PAL8 textures ---");
    for (k, range) in &by_key {
        let d = &data[range.clone()];
        let Some(ti) = parse_texture(d) else { continue };
        if ti.format != 1 {
            continue;
        }
        let prefix = if d.len() >= 56 {
            u32::from_le_bytes([d[52], d[53], d[54], d[55]])
        } else {
            0
        };
        let hit = by_key.get(&prefix).map(|r| {
            let pd = &data[r.clone()];
            pd.len() == 1024 && !is_texture_entry(pd)
        });
        println!(
            "tex {:#x} {}x{} prefix={:#x} same-sector-palette={:?}",
            k, ti.width, ti.height, prefix, hit
        );
        if hit == Some(true) {
            let pd = &data[by_key[&prefix].clone()];
            print!("  palette first 8 RGBA:");
            for px in pd.chunks_exact(4).take(8) {
                print!(" ({},{},{},{})", px[0], px[1], px[2], px[3]);
            }
            println!();
        }
    }
    ExitCode::SUCCESS
}
