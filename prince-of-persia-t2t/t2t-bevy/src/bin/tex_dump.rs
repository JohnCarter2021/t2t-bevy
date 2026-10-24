//! Debug: dump all texture records sharing a key in a sector.
//! usage: tex_dump <bf> <sector-substr> <key-hex>...
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::texture::parse_texture;
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
    let records = walk_wow(&data);
    for khex in &args[3..] {
        let key = u32::from_str_radix(khex.trim_start_matches("0x"), 16).expect("hex");
        let matches: Vec<_> = records.iter().filter(|r| r.key == key).collect();
        println!("key {khex}: {} record(s)", matches.len());
        for r in &matches {
            let sub = &data[r.data_range()];
            let ti = parse_texture(sub);
            println!(
                "  len={} parsed={:?}",
                sub.len(),
                ti.map(|t| (t.format, t.width, t.height, t.pix_start))
            );
        }
    }
    ExitCode::SUCCESS
}
