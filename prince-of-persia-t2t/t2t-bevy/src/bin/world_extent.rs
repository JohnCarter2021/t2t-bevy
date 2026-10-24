//! world_extent — sample the collision floor height on a
//! grid around a point, to diagnose fall-through spawns.
//! Usage: world_extent <bf> <sector-substr> <cx> <cy>
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
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&substr))
        .expect("sector not found");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let (mn, mx) = world.aabb();
    let center = [
        (mn[0] + mx[0]) / 2.0,
        (mn[1] + mx[1]) / 2.0,
        (mn[2] + mx[2]) / 2.0,
    ];
    let extent = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]).max(1e-6);
    let s = 40.0 / extent;
    println!("jade aabb=[{:.1},{:.1},{:.1}]..[{:.1},{:.1},{:.1}] center={:.1?} extent={:.1} s={:.4}", mn[0], mn[1], mn[2], mx[0], mx[1], mx[2], center, extent, s);
    let mut gmin = [f32::INFINITY; 3];
    let mut gmax = [f32::NEG_INFINITY; 3];
    let mut n = 0usize;
    for p in &world.placed {
        for v in p.world_vertices.chunks_exact(3) {
            let b = [
                (v[0] - center[0]) * s,
                (v[2] - center[2]) * s,
                (v[1] - center[1]) * s,
            ];
            for i in 0..3 {
                gmin[i] = gmin[i].min(b[i]);
                gmax[i] = gmax[i].max(b[i]);
            }
            n += 1;
        }
    }
    println!("game-coord vertex aabb min={:.2?} max={:.2?} verts={}", gmin, gmax, n);
    println!("groups={} placed={}", world.textures.len(), world.placed.len());
}
