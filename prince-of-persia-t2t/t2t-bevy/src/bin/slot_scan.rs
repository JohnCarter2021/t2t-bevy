//! Scan Jade behavior records for the slot-store idiom
//! (slot, 21.500) (K, 21.100) — two consecutive pairs where
//! the first pair's operand is a frame slot and the second
//! pair's operand is a plain int K. In the movement
//! records, (4, 21.500) (K, 21.100) stores K into slot 4
//! (the action return); (0, 21.500) (K, 21.100) is the
//! parallel compare/load shape. Prints record key, cell
//! index, slot, K.
//! usage: slot_scan <bf> [record-key-hex]
//! With no key, scans every record of the AMIR sector.
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
    let want = args
        .get(2)
        .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("key hex"));
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    for r in walk_wow(&data) {
        if let Some(w) = want {
            if r.key != w {
                continue;
            }
        }
        if r.size < 20 {
            continue;
        }
        let body = r.body_off;
        let rec_end = (body + r.size as usize).min(data.len());
        if body + 16 >= rec_end {
            continue;
        }
        let pool = body + le32(&data, body + 4) as usize;
        let end = pool.min(rec_end);
        if end <= body + 16 {
            continue;
        }
        let ncells = (end - (body + 16)) / 4;
        let cell = |i: usize| -> (u16, u16) {
            let w = le32(&data, body + 16 + i * 4);
            ((w & 0xffff) as u16, (w >> 16) as u16)
        };
        for i in 0..ncells.saturating_sub(3) {
            let (id0, ty0) = cell(i);
            let (id1, ty1) = cell(i + 1);
            let (id2, ty2) = cell(i + 2);
            let (id3, ty3) = cell(i + 3);
            if ty0 == 0 && ty1 == 0x0500 && id1 == 0x21 && ty2 == 0 && ty3 == 0x0100 && id3 == 0x21
            {
                println!(
                    "rec {:#010x} cell {}: store slot {} <- {}",
                    r.key, i, id0, id2
                );
            }
        }
    }
    ExitCode::SUCCESS
}
