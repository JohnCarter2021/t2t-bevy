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
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&substr))
        .expect("sector not found");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let soup = TriangleSoup::from_world(&world);
    let (mn, mx) = world.aabb();
    println!(
        "sector={} aabb=[{:.0},{:.0},{:.0}]..[{:.0},{:.0},{:.0}]",
        e.name, mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]
    );
    for dy in (-20..=20).step_by(10) {
        for dx in (-20..=20).step_by(10) {
            let (x, y) = (cx + dx as f32, cy + dy as f32);
            let h = soup.ground_height(x, y, 400.0, 30.0);
            match h {
                Some(z) => println!("({x:>7.1},{y:>7.1}) floor={z:.2}"),
                None => println!("({x:>7.1},{y:>7.1}) floor=NONE"),
            }
        }
    }
}
