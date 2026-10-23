//! Debug: why does find_pole reject everything in a
//! sector? Replicates the bucketing and prints cluster
//! statistics BEFORE the acceptance filters.
use std::collections::HashMap;
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
    let ctr = [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0, (mn[2] + mx[2]) / 2.0];
    println!("soup {} tris, center {:?}", soup.tris.len(), ctr);

    // Bucket ALL wall centroids (no distance/z window).
    let mut cells: HashMap<(i32, i32), (Vec<[f32; 2]>, f32, f32)> = HashMap::new();
    let mut walls = 0usize;
    for t in &soup.tris {
        let (ax, ay, az) = (t[0], t[1], t[2]);
        let (bx, by, bz) = (t[3], t[4], t[5]);
        let (cx, cy, cz) = (t[6], t[7], t[8]);
        let (ux, uy, uz) = (bx - ax, by - ay, bz - az);
        let (vx, vy, vz) = (cx - ax, cy - ay, cz - az);
        let nz = ux * vy - uy * vx;
        let nl = ((uy * vz - uz * vy).powi(2) + (uz * vx - ux * vz).powi(2) + nz * nz).sqrt();
        if nl < 1e-9 || (nz / nl).abs() >= 0.5 {
            continue;
        }
        walls += 1;
        let cen = [(ax + bx + cx) / 3.0, (ay + by + cy) / 3.0, (az + bz + cz) / 3.0];
        let key = ((cen[0] / 30.0).floor() as i32, (cen[1] / 30.0).floor() as i32);
        let e = cells.entry(key).or_insert((Vec::new(), f32::INFINITY, f32::NEG_INFINITY));
        e.0.push([cen[0], cen[1]]);
        e.1 = e.1.min(az.min(bz).min(cz));
        e.2 = e.2.max(az.max(bz).max(cz));
    }
    println!("wall tris {walls}, cells {}", cells.len());

    // Per-cell span histogram: how tall are single cells?
    let mut tall_cells = 0;
    for (_, (_, mnz, mxz)) in &cells {
        if mxz - mnz >= 180.0 {
            tall_cells += 1;
        }
    }
    println!("cells with span>=180: {tall_cells}");

    // Biggest-span bounded clusters, stats printed raw.
    let keys: Vec<(i32, i32)> = cells.keys().copied().collect();
    let mut assigned: std::collections::HashSet<(i32, i32)> = Default::default();
    let mut rows: Vec<(f32, usize, f32, f32, usize, f32)> = Vec::new();
    for start in keys {
        if assigned.contains(&start) {
            continue;
        }
        let mut stack = vec![start];
        assigned.insert(start);
        let mut cens: Vec<[f32; 2]> = Vec::new();
        let (mut mnz, mut mxz) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut kx0, mut kx1, mut ky0, mut ky1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
        while let Some(k) = stack.pop() {
            let c = &cells[&k];
            cens.extend_from_slice(&c.0);
            mnz = mnz.min(c.1);
            mxz = mxz.max(c.2);
            kx0 = kx0.min(k.0);
            kx1 = kx1.max(k.0);
            ky0 = ky0.min(k.1);
            ky1 = ky1.max(k.1);
            for dk in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let nk = (k.0 + dk.0, k.1 + dk.1);
                let dcx = (nk.0 - start.0) as f32 * 30.0;
                let dcy = (nk.1 - start.1) as f32 * 30.0;
                if dcx * dcx + dcy * dcy > 105.0 * 105.0 {
                    continue;
                }
                if cells.contains_key(&nk) && !assigned.contains(&nk) {
                    assigned.insert(nk);
                    stack.push(nk);
                }
            }
        }
        let n = cens.len();
        if n < 4 {
            continue;
        }
        let center = [
            cens.iter().map(|c| c[0]).sum::<f32>() / n as f32,
            cens.iter().map(|c| c[1]).sum::<f32>() / n as f32,
        ];
        let mut quads = [0usize; 4];
        let mut rsum = 0.0f32;
        for c in &cens {
            let ox = c[0] - center[0];
            let oy = c[1] - center[1];
            rsum += (ox * ox + oy * oy).sqrt();
            let q = (if ox >= 0.0 { 1 } else { 0 }) + (if oy >= 0.0 { 2 } else { 0 });
            quads[q] += 1;
        }
        let ringed = quads.iter().filter(|&&q| q >= 2).count();
        let ext = ((kx1 - kx0 + 1).max(ky1 - ky0 + 1)) as f32 * 30.0;
        rows.push((mxz - mnz, n, ext, ringed as f32, 0, rsum / n as f32));
        let _ = center;
    }
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("top clusters by span (span, n, extent, ringed, mean_r):");
    for r in rows.iter().take(10) {
        println!("  span {:.0}  n {}  ext {:.0}  ringed {}  mean_r {:.1}", r.0, r.1, r.2, r.3, r.5);
    }
    ExitCode::SUCCESS
}
