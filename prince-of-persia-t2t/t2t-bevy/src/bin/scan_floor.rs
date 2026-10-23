//! Debug: print ground heights on a grid around a point.
//! usage: scan_floor <bf> <sector-substr> <cx> <cy>
use std::process::ExitCode;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::collision::TriangleSoup;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let entry = bf
        .active_files()
        .iter()
        .find(|f| f.name.contains(&args[2]))
        .expect("entry")
        .clone();
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let soup = TriangleSoup::from_world(&world);
    let cx: f32 = args[3].parse().unwrap();
    let cy: f32 = args[4].parse().unwrap();
    let z_ref: f32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(400.0);
    print!("     y\\x");
    for gx in -10..=10 {
        print!(" {:>6.0}", cx + gx as f32 * 60.0);
    }
    println!();
    for gy in -10..=10 {
        let y = cy + gy as f32 * 60.0;
        print!("{:>7.0}", y);
        for gx in -10..=10 {
            let x = cx + gx as f32 * 60.0;
            match soup.ground_height(x, y, z_ref, 30.0) {
                Some(z) => print!(" {:>6.0}", z),
                None => print!("      ."),
            }
        }
        println!();
    }
    ExitCode::SUCCESS
}
