//! geo_tex — list a sector's placed GEOs with their
//! element texture keys, to identify which groups render.
//! Usage: geo_tex <bf> <sector-substr> <name-substr>
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage");
    let substr = std::env::args().nth(2).expect("usage");
    let want = std::env::args().nth(3).expect("usage").to_lowercase();
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&substr))
        .expect("sector not found");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    for p in &world.placed {
        if p.gao_name.to_lowercase().contains(&want) {
            let faces = p.geo.faces.len();
            let texs: Vec<String> = p
                .element_textures
                .iter()
                .map(|t| match t {
                    Some(k) => format!("{k:#010x}"),
                    None => "None".to_string(),
                })
                .collect();
            println!("{} faces={} elem_tex=[{}]", p.gao_name, faces, texs.join(","));
        }
    }
    println!("--- sector texture keys (decoded map):");
    for (k, _) in world.textures.iter() {
        println!("  {k:#010x}");
    }
}
