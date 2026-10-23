//! Debug: scan a sector for climbable poles.
//! usage: pole_probe <bf> <sector-substr> <cx> <cy> <cz>
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
    let (mn, mx) = world.aabb();
    println!("world AABB {:?} .. {:?}, {} tris in soup", mn, mx, soup.tris.len());
    let auto = args[3] == "auto";
    let c: [f32; 3] = if auto {
        [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0, (mn[2] + mx[2]) / 2.0]
    } else {
        [
            args[3].parse().unwrap(),
            args[4].parse().unwrap(),
            args[5].parse().unwrap(),
        ]
    };
    let (span_x, span_y, step) = if auto {
        ((mx[0] - mn[0]) / 2.0, (mx[1] - mn[1]) / 2.0, 120.0f32)
    } else {
        (960.0, 960.0, 60.0f32)
    };
    let nx = (span_x / step) as i32;
    let ny = (span_y / step) as i32;
    let mut found: Vec<(i32, i32, f32, f32, f32)> = Vec::new(); // cx, cy, top, bottom, dist
    for gx in -nx..=nx {
        for gy in -ny..=ny {
            let mut hit = None;
            for pz in [c[2], c[2] - 120.0] {
                let p = [c[0] + gx as f32 * step, c[1] + gy as f32 * step, pz];
                if let Some(pole) = soup.find_pole(p, 175.0) {
                    hit = Some(pole);
                    break;
                }
            }
            if let Some(pole) = hit {
                let key = (
                    (pole.center[0] / 60.0).round() as i32,
                    (pole.center[1] / 60.0).round() as i32,
                );
                if found.iter().any(|f| f.0 == key.0 && f.1 == key.1) {
                    continue;
                }
                let d = ((pole.center[0] - c[0]).powi(2) + (pole.center[1] - c[1]).powi(2)).sqrt();
                found.push((key.0, key.1, pole.top, pole.bottom, d));
                println!(
                    "pole at ({:.0}, {:.0}) span {:.0}..{:.0}, dist {:.0} from probe center",
                    pole.center[0], pole.center[1], pole.bottom, pole.top, d
                );
            }
        }
    }
    println!("{} distinct poles", found.len());
    ExitCode::SUCCESS
}
