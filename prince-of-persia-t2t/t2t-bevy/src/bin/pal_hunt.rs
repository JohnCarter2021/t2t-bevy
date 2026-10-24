//! Debug: list palette-sized records in a sector (possible
//! PAL8 palettes: 256 x RGBA = 1024 bytes payload).
//! usage: pal_hunt <bf> <sector-substr>
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
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
    for r in walk_wow(&data) {
        let len = r.data_range().len();
        if (1000..=1100).contains(&len) {
            let sub = &data[r.data_range()];
            // Count entries with alpha 255 (plausible RGBA palette).
            let mut opaque = 0;
            for px in sub.chunks_exact(4) {
                if px[3] == 255 {
                    opaque += 1;
                }
            }
            println!(
                "key {:#x} len={} tag={} gro={:?} alpha255={}/{}",
                r.key,
                len,
                r.tag,
                r.gro_type,
                opaque,
                sub.len() / 4
            );
        }
    }
    ExitCode::SUCCESS
}
