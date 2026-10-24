//! List the 0x0001-type GAM sibling records in AMIR:
//! records whose first u32 has low16 == 0x0001 (vs 0xff01
//! for normal bank descriptors), with their +4/+8 words
//! (packed clip-key halves) and size.
//! usage: gam_scan <bf>
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    for r in walk_wow(&data) {
        if r.size < 52 {
            continue;
        }
        let body = r.body_off;
        if body + 52 > data.len() {
            continue;
        }
        let w0 = le32(&data, body);
        if w0 & 0xffff != 0x0001 {
            continue;
        }
        println!(
            "rec {:#010x} size {} type {:#010x} +4 {:#010x} +8 {:#010x} +34f {}",
            r.key,
            r.size,
            w0,
            le32(&data, body + 4),
            le32(&data, body + 8),
            f32::from_bits(le32(&data, body + 34))
        );
    }
    ExitCode::SUCCESS
}
