//! Debug: max horizontal ring-hit count over a sector
//! grid at a given height — tells whether ANY point is
//! surrounded by near walls (a column interior) at all.
use std::process::ExitCode;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::collision::TriangleSoup;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn is_wall(t: &[f32; 9]) -> bool {
    let (ux, uy, uz) = (t[3] - t[0], t[4] - t[1], t[5] - t[2]);
    let (vx, vy, vz) = (t[6] - t[0], t[7] - t[1], t[8] - t[2]);
    let nz = ux * vy - uy * vx;
    let nl = ((uy * vz - uz * vy).powi(2) + (uz * vx - ux * vz).powi(2) + nz * nz).sqrt();
    nl >= 1e-9 && (nz / nl).abs() < 0.5
}

fn ray(tris: &[[f32; 9]], c: [f32; 2], z: f32, d: [f32; 2]) -> Option<f32> {
    let mut best: Option<f32> = None;
    for t in tris {
        let (z0, z1, z2) = (t[2], t[5], t[8]);
        if z < z0.min(z1).min(z2) || z > z0.max(z1).max(z2) {
            continue;
        }
        let e1 = [t[3] - t[0], t[4] - t[1], t[5] - t[2]];
        let e2 = [t[6] - t[0], t[7] - t[1], t[8] - t[2]];
        let p = [d[1] * e2[2], -d[0] * e2[2], d[0] * e2[1] - d[1] * e2[0]];
        let det = e1[0] * p[0] + e1[1] * p[1] + e1[2] * p[2];
        if det.abs() < 1e-9 {
            continue;
        }
        let inv = 1.0 / det;
        let tv = [c[0] - t[0], c[1] - t[1], z - t[2]];
        let u = (tv[0] * p[0] + tv[1] * p[1] + tv[2] * p[2]) * inv;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = [
            tv[1] * e1[2] - tv[2] * e1[1],
            tv[2] * e1[0] - tv[0] * e1[2],
            tv[0] * e1[1] - tv[1] * e1[0],
        ];
        let v = (d[0] * q[0] + d[1] * q[1]) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let tt = (e2[0] * q[0] + e2[1] * q[1] + e2[2] * q[2]) * inv;
        if tt > 4.0 && tt < 130.0 && best.map_or(true, |b| tt < b) {
            best = Some(tt);
        }
    }
    best
}

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
    let walls: Vec<[f32; 9]> = soup.tris.iter().filter(|t| is_wall(t)).copied().collect();
    let (mn, mx) = world.aabb();
    let z: f32 = args[3].parse().unwrap();
    const D: [[f32; 2]; 8] = [
        [1.0, 0.0], [0.7071, 0.7071], [0.0, 1.0], [-0.7071, 0.7071],
        [-1.0, 0.0], [-0.7071, -0.7071], [0.0, -1.0], [0.7071, -0.7071],
    ];
    let mut best = (0usize, [0.0f32; 2]);
    let mut ge5 = 0usize;
    let mut y = mn[1];
    while y < mx[1] {
        let mut x = mn[0];
        while x < mx[0] {
            // Cheap prune: only points near some wall tri.
            let near = walls.iter().any(|t| {
                let cx = (t[0] + t[3] + t[6]) / 3.0 - x;
                let cy = (t[1] + t[4] + t[7]) / 3.0 - y;
                cx * cx + cy * cy < 200.0 * 200.0
            });
            if near {
                let local: Vec<[f32; 9]> = walls
                    .iter()
                    .filter(|t| {
                        let cx = (t[0] + t[3] + t[6]) / 3.0 - x;
                        let cy = (t[1] + t[4] + t[7]) / 3.0 - y;
                        cx * cx + cy * cy < 260.0 * 260.0
                    })
                    .copied()
                    .collect();
                let n = (0..8).filter(|&k| ray(&local, [x, y], z, D[k]).is_some()).count();
                if n >= 5 {
                    ge5 += 1;
                }
                if n > best.0 {
                    best = (n, [x, y]);
                }
            }
            x += 30.0;
        }
        y += 30.0;
    }
    println!("z={z}: max ring hits {} at {:?}; points with >=5 hits: {}", best.0, best.1, ge5);
    if best.0 > 0 {
        let local: Vec<[f32; 9]> = walls
            .iter()
            .filter(|t| {
                let cx = (t[0] + t[3] + t[6]) / 3.0 - best.1[0];
                let cy = (t[1] + t[4] + t[7]) / 3.0 - best.1[1];
                cx * cx + cy * cy < 300.0 * 300.0
            })
            .copied()
            .collect();
        let ds: Vec<String> = (0..8)
            .map(|k| match ray(&local, best.1, z, D[k]) {
                Some(t) => format!("{t:.0}"),
                None => "--".to_string(),
            })
            .collect();
        println!("  hit distances at best point: {}", ds.join(" "));
    }
    ExitCode::SUCCESS
}
