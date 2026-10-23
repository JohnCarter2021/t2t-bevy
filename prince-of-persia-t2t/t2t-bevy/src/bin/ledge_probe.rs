//! ledge_probe — scan around a Jade point for grabbable
//! ledges (staging traversal demos). Usage:
//! ledge_probe <bf> <sector-substr> <x> <y>

use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::collision::TriangleSoup;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("bf");
    let e = bf.active_files().into_iter().find(|e| e.name.contains(&args[2])).expect("sector");
    let data = decompress_entry(bf.read_data(e.index)).expect("data");
    let recs = walk_wow(&data);
    let world = assemble(&data, &recs);
    let soup = TriangleSoup::from_world(&world);
    if args[3] == "beam" {
        // Whole-sector beam scan: grid points with a floor,
        // both perpendiculars.
        let (mn, mx) = world.aabb();
        let mut hits = 0;
        let mut x = mn[0] + 50.0;
        while x < mx[0] - 50.0 && hits < 25 {
            let mut y = mn[1] + 50.0;
            while y < mx[1] - 50.0 && hits < 25 {
                if let Some(g) = soup.ground_height(x, y, 1.0e5, 0.0) {
                    for perp in [[1.0, 0.0], [0.0, 1.0]] {
                        if let Some(b) = soup.beam_span([x, y, g], perp) {
                            println!("beam at ({x:.0},{y:.0},{g:.0}) width {:.0} axis {:?}", b.width, b.axis);
                            hits += 1;
                        }
                    }
                }
                y += 50.0;
            }
            x += 50.0;
        }
        println!("beam hits: {hits}");
        return;
    }
    let cx: f32 = args[3].parse().unwrap();
    let cy: f32 = args[4].parse().unwrap();
    let dirs = [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]];
    for rad in [40.0f32, 80.0, 120.0, 200.0, 300.0] {
        for a in 0..16 {
            let th = a as f32 * std::f32::consts::TAU / 16.0;
            let (px, py) = (cx + rad * th.cos(), cy + rad * th.sin());
            for &dir in &dirs {
                for &hz in &[62.0f32, 150.0, 300.0, 600.0, 1000.0, 1600.0, 2400.0] {
                    if let Some(l) = soup.find_ledge([px, py, hz - 170.0], dir, hz) {
                        println!(
                            "probe ({px:.0},{py:.0}) z={:.0} dir={dir:?} -> ledge top {:?} n {:?}",
                            hz - 170.0, l.top, l.normal
                        );
                    }
                }
            }
        }
    }
}
