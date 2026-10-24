//! floor_probe — sample the collision floor height on a
//! grid around a point, to diagnose fall-through spawns.
//! Usage: floor_probe <bf> <sector-substr> <cx> <cy>
//! Prints ground_height at (cx,cy) and at offsets
//! -20..20 step 10 in x and y, from z_ref=400.
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::collision::TriangleSoup;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage");
    let substr = std::env::args().nth(2).expect("usage");
    let cx: f32 = std::env::args().nth(3).expect("usage").parse().expect("cx");
    let cy: f32 = std::env::args().nth(4).expect("usage").parse().expect("cy");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf.active_files().into_iter().find(|e| e.name.contains(&substr)).expect("sector");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let soup = TriangleSoup::from_world(&world);
    for (zr, su) in [(56.973, 0.0), (56.973, 30.0), (400.0, 30.0), (56.975, 30.0)] {
        println!("ground_height({cx},{cy}, z_ref={zr}, step={su}) = {:?}", soup.ground_height(cx, cy, zr, su));
    }
    // every walkable triangle covering (cx,cy): its interpolated z
    for t in &soup.tris {
        let (ax, ay, az) = (t[0], t[1], t[2]);
        let (bx, by, bz) = (t[3], t[4], t[5]);
        let (cx2, cy2, cz) = (t[6], t[7], t[8]);
        let ux = [bx - ax, by - ay, bz - az];
        let vx = [cx2 - ax, cy2 - ay, cz - az];
        let nz = ux[0] * vx[1] - ux[1] * vx[0];
        let nxy = ux[1] * vx[2] - ux[2] * vx[1];
        let nyx = ux[2] * vx[0] - ux[0] * vx[2];
        let nlen = (nz * nz + nxy * nxy + nyx * nyx).sqrt();
        if nlen < 1e-9 || nz / nlen < 0.5 { continue; }
        let denom = (by - cy2) * (ax - cx2) + (cx2 - bx) * (ay - cy2);
        if denom.abs() < 1e-9 { continue; }
        let w0 = ((by - cy2) * (cx - cx2) + (cx2 - bx) * (cy - cy2)) / denom;
        let w1 = ((cy2 - ay) * (cx - cx2) + (ax - cx2) * (cy - cy2)) / denom;
        let w2 = 1.0 - w0 - w1;
        if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 { continue; }
        let z = w0 * az + w1 * bz + w2 * cz;
        println!("  covering walkable tri: z={z:.3} verts=({ax:.1},{ay:.1},{az:.1}) ({bx:.1},{by:.1},{bz:.1}) ({cx2:.1},{cy2:.1},{cz:.1})");
    }
}
