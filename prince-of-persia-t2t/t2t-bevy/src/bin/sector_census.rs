//! Campaign sector census: for each sector name in a
//! list file (one per line), find matching BIG entries
//! by substring (the same lookup t2t_game uses), then
//! decompress + assemble each match and report geometry
//! stats. Unmatched "_wow" entries are listed at the end
//! (non-campaign sectors: menus, cinematics, GFX, Quai).
//! usage: sector_census <bf> <sector-list-file>
use std::process::ExitCode;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let list = std::fs::read_to_string(&args[2]).expect("sector list");
    let sectors: Vec<&str> = list.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let files = bf.active_files();
    let mut matched_names: Vec<String> = Vec::new();
    for s in &sectors {
        let hits: Vec<_> = files.iter().filter(|e| e.name.contains(s)).collect();
        if hits.is_empty() {
            println!("{s}: NO ENTRY");
            continue;
        }
        for e in hits {
            matched_names.push(e.name.clone());
            let data = match decompress_entry(bf.read_data(e.index)) {
                Ok(d) => d,
                Err(_) => {
                    println!("{s}: {} DECOMPRESS-FAIL", e.name);
                    continue;
                }
            };
            let records = walk_wow(&data);
            let world = assemble(&data, &records);
            let faces: usize = world.placed.iter().map(|p| p.geo.faces.len()).sum();
            let (mn, mx) = if world.placed.is_empty() {
                ([0.0; 3], [0.0; 3])
            } else {
                world.aabb()
            };
            println!(
                "{s}: {} placed={} faces={} texkeys={} aabb_min=[{:.0},{:.0},{:.0}] aabb_max=[{:.0},{:.0},{:.0}]",
                e.name,
                world.placed.len(),
                faces,
                world.textures.len(),
                mn[0],
                mn[1],
                mn[2],
                mx[0],
                mx[1],
                mx[2]
            );
        }
    }
    println!("--- wow entries matched by NO sector name:");
    for e in &files {
        if e.name.contains("_wow") && !matched_names.contains(&e.name) {
            println!("  {}", e.name);
        }
    }
    ExitCode::SUCCESS
}
