//! Find callers of Jade library functions: scan every AMIR
//! behavior record for call cells (fn id, type 0x1000)
//! matching the given fn ids, and print the surrounding
//! cells for context (what flows into the call).
//! usage: call_scan <bf> <fn-hex>...
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn fmt_cell(w: u32) -> String {
    let (id, ty) = ((w & 0xffff) as u16, (w >> 16) as u16);
    match ty {
        0x0000 => format!("{id}"),
        0x0200 => format!("var:{id:x}"),
        0x0300 => format!("r3:{id:x}"),
        0x1b00 | 0x1000 | 0x1c00 => format!("fn:{id:x}"),
        _ => format!("{id:x}.{ty:x}"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let targets: Vec<u16> = args[2..]
        .iter()
        .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("fn hex") as u16)
        .collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    for r in walk_wow(&data) {
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
        let cell = |i: usize| -> u32 { le32(&data, body + 16 + i * 4) };
        for i in 0..ncells {
            let w = cell(i);
            let (id, ty) = ((w & 0xffff) as u16, (w >> 16) as u16);
            if ty == 0x1000 && targets.contains(&id) {
                let lo = i.saturating_sub(4);
                let hi = (i + 5).min(ncells);
                let ctx: Vec<String> = (lo..hi).map(|j| fmt_cell(cell(j))).collect();
                println!(
                    "rec {:#010x} cell {}: call fn:{:x} ... {}",
                    r.key,
                    i,
                    id,
                    ctx.join(" ")
                );
            }
        }
    }
    ExitCode::SUCCESS
}
