//! Menu3D composition probe: Jade AABB + vertex
//! distribution, to frame the menu camera from data.
use std::process::ExitCode;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let e = bf.active_files().into_iter()
        .find(|e| e.name.contains("Menu3D")).expect("menu entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let (mn, mx) = world.aabb();
    println!("jade aabb min {mn:?} max {mx:?}");
    let center = [(mn[0]+mx[0])/2.0, (mn[1]+mx[1])/2.0, (mn[2]+mx[2])/2.0];
    let extent = (mx[0]-mn[0]).max(mx[1]-mn[1]).max(mx[2]-mn[2]);
    let s = 40.0 / extent;
    println!("bevy scale {s:.5}; bevy y = (jade z - {:.1}) * s", center[2]);
    // Histogram of bevy-space y over all world verts, and
    // the XZ centroid of the lowest third (island mass).
    let mut buckets = [0usize; 16];
    let (mut cx, mut cz, mut cn) = (0f64, 0f64, 0usize);
    let y_lo = (mn[2]-center[2])*s;
    let y_hi = (mx[2]-center[2])*s;
    for p in &world.placed {
        for v in p.world_vertices.chunks_exact(3) {
            let by = (v[2]-center[2])*s;
            let b = ((by - y_lo) / (y_hi - y_lo) * 15.99) as usize;
            buckets[b.min(15)] += 1;
            if by < y_lo + (y_hi-y_lo)/3.0 {
                cx += ((v[0]-center[0])*s) as f64;
                cz += ((v[1]-center[1])*s) as f64;
                cn += 1;
            }
        }
    }
    for (i, c) in buckets.iter().enumerate() {
        let y = y_lo + (y_hi-y_lo) * i as f32 / 16.0;
        println!("  y {y:6.2} .. : {c}");
    }
    if cn > 0 {
        println!("lowest-third centroid: x {:.2} z {:.2} ({} verts)",
            cx/cn as f64, cz/cn as f64, cn);
    }
    ExitCode::SUCCESS
}
