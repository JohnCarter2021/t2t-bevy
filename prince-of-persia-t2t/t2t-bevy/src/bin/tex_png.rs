//! Debug: export one texture record (last occurrence) to PNG.
//! usage: tex_png <bf> <sector-substr> <key-hex> <out-png>
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::png::write_png_rgba;
use t2t::texture::{decode_texture, parse_texture};
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
    let key = u32::from_str_radix(args[3].trim_start_matches("0x"), 16).expect("hex");
    let rec = records
        .iter()
        .rev()
        .find(|r| r.key == key)
        .expect("key not found");
    let sub = &data[rec.data_range()];
    let ti = parse_texture(sub).expect("not a texture");
    let rgba = decode_texture(sub, &ti);
    println!(
        "key {:#x}: fmt={} {}x{} rgba={} bytes",
        key,
        ti.format,
        ti.width,
        ti.height,
        rgba.len()
    );
    if !rgba.is_empty() {
        write_png_rgba(std::path::Path::new(&args[4]), ti.width, ti.height, &rgba)
            .expect("write png");
        println!("wrote {}", args[4]);
    }
    ExitCode::SUCCESS
}
