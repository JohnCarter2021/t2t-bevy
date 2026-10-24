//! Debug: dump a grm chain's raw words. usage: grm_dump <bf> <sector> <grm-hex>
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::material::parse_material;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let entry = bf.active_files().into_iter().find(|e| e.name.contains(&args[2])).expect("entry");
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    let records = walk_wow(&data);
    let grm = u32::from_str_radix(args[3].trim_start_matches("0x"), 16).expect("hex");
    let rec = records.iter().find(|r| r.key == grm).expect("grm record");
    let sub = &data[rec.data_range()];
    println!("grm {:#x} gro={:?} len={}", grm, rec.gro_type, sub.len());
    println!("  parsed: {:?}", parse_material(sub, rec.gro_type.unwrap_or(0)));
    let words: Vec<u32> = sub.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    for (i, w) in words.iter().enumerate() {
        println!("  [{:3}] {:#010x}", i, w);
    }
    // Sub-materials (kind 2 = multi).
    if let Some(m) = parse_material(sub, rec.gro_type.unwrap_or(0)) {
        for sk in &m.sub_material_keys {
            if let Some(sr) = records.iter().find(|r| r.key == *sk) {
                let sd = &data[sr.data_range()];
                let sw: Vec<u32> = sd.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                println!("  sub {:#x} gro={:?} len={} words={:?}", sk, sr.gro_type, sd.len(), &sw[..sw.len().min(20)]);
            } else {
                println!("  sub {:#x} NOT in sector", sk);
            }
        }
    }
    ExitCode::SUCCESS
}
