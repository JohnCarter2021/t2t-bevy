//! World collision: a triangle soup built from the assembled
//! world, with ground-height queries for the character
//! controller. Jade space (Z-up, ~100 units per metre).

use crate::assemble::AssembledWorld;

/// A narrow support found by `TriangleSoup::beam_span`.
#[derive(Debug, Clone, Copy)]
pub struct BeamSpan {
    /// Centerline point nearest the query position.
    pub center: [f32; 2],
    /// Unit vector along the beam.
    pub axis: [f32; 2],
    pub width: f32,
    pub z: f32,
}

/// A climbable pole found by `TriangleSoup::find_pole`.
#[derive(Debug, Clone, Copy)]
pub struct Pole {
    /// Column center in the XY plane.
    pub center: [f32; 2],
    /// Highest/lowest wall geometry of the column.
    pub top: f32,
    pub bottom: f32,
}

/// A ledge found by `TriangleSoup::find_ledge`.
#[derive(Debug, Clone, Copy)]
pub struct Ledge {
    /// Point on the floor just inside the edge (Jade space).
    pub top: [f32; 3],
    /// Horizontal unit normal pointing into open air.
    pub normal: [f32; 2],
}

/// Flat triangle soup in Jade world space.
pub struct TriangleSoup {
    /// Each triangle: 9 floats (ax, ay, az, bx, by, bz, cx, cy, cz).
    pub tris: Vec<[f32; 9]>,
}

impl TriangleSoup {
    pub fn from_world(world: &AssembledWorld) -> TriangleSoup {
        let mut tris = Vec::new();
        for placed in &world.placed {
            let wv = &placed.world_vertices;
            for f in &placed.geo.faces {
                let mut t = [0f32; 9];
                for k in 0..3 {
                    let o = f[k] as usize * 3;
                    t[k * 3] = wv[o];
                    t[k * 3 + 1] = wv[o + 1];
                    t[k * 3 + 2] = wv[o + 2];
                }
                tris.push(t);
            }
        }
        TriangleSoup { tris }
    }

    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }

    /// Push a vertical capsule (circle of `radius` in XY,
    /// spanning z_base..z_base+height) out of all wall
    /// triangles (normal cos to +Z < 0.5 whose z-range overlaps
    /// the capsule above step-over height). Returns the
    /// corrected XY. Two relaxation passes settle corners.
    pub fn resolve_walls(
        &self,
        xy: [f32; 2],
        radius: f32,
        z_base: f32,
        height: f32,
    ) -> [f32; 2] {
        let mut p = xy;
        let z_lo = z_base + 20.0; // step-over: curbs below this are ground's job
        let z_hi = z_base + height;
        for _pass in 0..2 {
            for t in &self.tris {
                let (ax, ay, az) = (t[0], t[1], t[2]);
                let (bx, by, bz) = (t[3], t[4], t[5]);
                let (cx, cy, cz) = (t[6], t[7], t[8]);
                let tz_min = az.min(bz).min(cz);
                let tz_max = az.max(bz).max(cz);
                if tz_max < z_lo || tz_min > z_hi {
                    continue;
                }
                let ux = [bx - ax, by - ay, bz - az];
                let vx = [cx - ax, cy - ay, cz - az];
                let nx = ux[1] * vx[2] - ux[2] * vx[1];
                let ny = ux[2] * vx[0] - ux[0] * vx[2];
                let nz = ux[0] * vx[1] - ux[1] * vx[0];
                let nlen = (nx * nx + ny * ny + nz * nz).sqrt();
                if nlen < 1e-9 || (nz / nlen).abs() >= 0.5 {
                    continue; // floor/ceiling, not a wall
                }
                // Face rule: near the wall plane with the
                // perpendicular foot inside the triangle -> push
                // along the wall's XY normal.
                let n2l = (nx * nx + ny * ny).sqrt();
                if n2l > 1e-9 {
                    let n2 = [nx / n2l, ny / n2l];
                    let s = (p[0] - ax) * n2[0] + (p[1] - ay) * n2[1];
                    if s.abs() < radius {
                        let f = [p[0] - n2[0] * s, p[1] - n2[1] * s];
                        let denom = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
                        if denom.abs() > 1e-9 {
                            let w0 = ((by - cy) * (f[0] - cx) + (cx - bx) * (f[1] - cy)) / denom;
                            let w1 = ((cy - ay) * (f[0] - cx) + (ax - cx) * (f[1] - cy)) / denom;
                            let w2 = 1.0 - w0 - w1;
                            if w0 >= -0.001 && w1 >= -0.001 && w2 >= -0.001 {
                                let push = radius - s.abs();
                                let sgn = if s >= 0.0 { 1.0 } else { -1.0 };
                                p = [p[0] + n2[0] * sgn * push, p[1] + n2[1] * sgn * push];
                            }
                        }
                    }
                }
                // Edge rule: closest point on each XY edge
                // segment; push out (corners and wall ends).
                let edges = [(ax, ay, bx, by), (bx, by, cx, cy), (cx, cy, ax, ay)];
                for (ex, ey, fx, fy) in edges {
                    let (dx, dy) = (fx - ex, fy - ey);
                    let l2 = dx * dx + dy * dy;
                    let tpar = if l2 < 1e-9 {
                        0.0
                    } else {
                        (((p[0] - ex) * dx + (p[1] - ey) * dy) / l2).clamp(0.0, 1.0)
                    };
                    let q = [ex + dx * tpar, ey + dy * tpar];
                    let (ox, oy) = (p[0] - q[0], p[1] - q[1]);
                    let d2 = ox * ox + oy * oy;
                    if d2 < radius * radius && d2 > 1e-9 {
                        let d = d2.sqrt();
                        p = [q[0] + ox / d * radius, q[1] + oy / d * radius];
                    }
                }
            }
        }
        p
    }

    /// Measure the walkable width across `perp` at `pos`
    /// (feet at pos.z). Returns the beam span when BOTH
    /// sides end at a floor edge within 70 units and the
    /// total width is under 100 — a narrow support (beam,
    /// plank, pipe) rather than a floor.
    pub fn beam_span(&self, pos: [f32; 3], perp: [f32; 2]) -> Option<BeamSpan> {
        let pl = (perp[0] * perp[0] + perp[1] * perp[1]).sqrt();
        if pl < 1e-6 {
            return None;
        }
        let p = [perp[0] / pl, perp[1] / pl];
        let floor_at = |x: f32, y: f32| -> bool {
            match self.ground_height(x, y, pos[2] + 10.0, 0.0) {
                Some(g) => (g - pos[2]).abs() < 25.0,
                None => false,
            }
        };
        if !floor_at(pos[0], pos[1]) {
            return None;
        }
        let mut edges = [0.0f32; 2];
        for (i, sgn) in [1.0f32, -1.0].iter().enumerate() {
            let mut d = 5.0f32;
            let mut found = false;
            while d <= 70.0 {
                if !floor_at(pos[0] + p[0] * sgn * d, pos[1] + p[1] * sgn * d) {
                    found = true;
                    break;
                }
                d += 5.0;
            }
            if !found {
                return None; // floor runs on: not a beam edge
            }
            edges[i] = d - 2.5;
        }
        let width = edges[0] + edges[1];
        if width >= 100.0 {
            return None;
        }
        let mid = (edges[0] - edges[1]) / 2.0;
        Some(BeamSpan {
            center: [pos[0] + p[0] * mid, pos[1] + p[1] * mid],
            axis: [-p[1], p[0]],
            width,
            z: pos[2],
        })
    }

    /// A climbable pole near `pos`, found by ray probing:
    /// a point is a pole axis when horizontal rays at
    /// several heights hit walls in at least 5 of 8
    /// directions within 75 units (a ring, >= 6 of 8 —
    /// a point ON a face only reaches 5 via corner hits). Real level
    /// meshes are too coarse for centroid clustering —
    /// their wall triangles are huge — so the detector
    /// probes geometry directly. Candidates are scanned
    /// on a 20-unit grid nearest-first, then centered by
    /// opposing-ray imbalance, the vertical span
    /// is walked while the ring holds.
    pub fn find_pole(&self, pos: [f32; 3], max_dist: f32) -> Option<Pole> {
        // Local wall triangles only (centroid window).
        let local: Vec<&[f32; 9]> = self
            .tris
            .iter()
            .filter(|t| {
                let cx = (t[0] + t[3] + t[6]) / 3.0 - pos[0];
                let cy = (t[1] + t[4] + t[7]) / 3.0 - pos[1];
                let cz = (t[2] + t[5] + t[8]) / 3.0;
                cx * cx + cy * cy < 260.0 * 260.0
                    && cz > pos[2] - 320.0
                    && cz < pos[2] + 720.0
                    && is_wall(t)
            })
            .collect();
        if local.is_empty() {
            return None;
        }
        // Candidate grid, nearest first.
        let step = 20.0f32;
        let n = (max_dist / step) as i32;
        let mut cands: Vec<(f32, [f32; 2])> = Vec::new();
        for i in -n..=n {
            for j in -n..=n {
                let c = [pos[0] + i as f32 * step, pos[1] + j as f32 * step];
                let d = ((c[0] - pos[0]).powi(2) + (c[1] - pos[1]).powi(2)).sqrt();
                if d <= max_dist {
                    cands.push((d, c));
                }
            }
        }
        cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for (_, c0) in cands {
            // Ring must hold at all 3 probe heights
            // (mid-column; near the caps the grip clamp
            // forbids holding anyway).
            let mut ok = 0;
            for dz in [-100.0, 0.0, 100.0] {
                if ring_hits(&local, c0, pos[2] + dz).is_some() {
                    ok += 1;
                }
            }
            if ok < 3 {
                continue;
            }
            // Center by opposing-ray imbalance.
            let mut c = c0;
            for _ in 0..3 {
                match ring_hits(&local, c, pos[2]) {
                    Some(hits) => {
                        let mut shift = [0.0f32; 2];
                        let mut pairs = 0;
                        for k in 0..4 {
                            let (t1, t2) = (hits[k], hits[k + 4]);
                            if let (Some(a), Some(b)) = (t1, t2) {
                                let d = dir8(k);
                                let s = (a - b) / 2.0;
                                shift[0] += d[0] * s;
                                shift[1] += d[1] * s;
                                pairs += 1;
                            }
                        }
                        if pairs == 0 {
                            break;
                        }
                        c[0] += shift[0] / pairs as f32;
                        c[1] += shift[1] / pairs as f32;
                    }
                    None => break,
                }
            }
            if ring_hits(&local, c, pos[2]).is_none() {
                continue;
            }
            // Walk the span while the ring holds.
            let mut top = pos[2];
            let mut z = pos[2];
            while z < pos[2] + 640.0 {
                z += 40.0;
                if ring_hits(&local, c, z).is_some() {
                    top = z;
                } else {
                    break;
                }
            }
            let mut bottom = pos[2];
            let mut z = pos[2];
            while z > pos[2] - 400.0 {
                z -= 40.0;
                if ring_hits(&local, c, z).is_some() {
                    bottom = z;
                } else {
                    break;
                }
            }
            let (top, bottom) = (top + 20.0, bottom - 20.0);
            if top - bottom < 150.0 {
                continue;
            }
            return Some(Pole { center: c, top, bottom });
        }
        None
    }

    /// A grabbable ledge: the top edge of a raised floor.
    /// `top` is a point on the floor just inside the edge;
    /// `normal` is the horizontal direction from the wall
    /// face back into open air (toward the hanging body).
    pub fn find_ledge(
        &self,
        pos: [f32; 3],
        dir: [f32; 2],
        hand_z: f32,
    ) -> Option<Ledge> {
        // March along dir; a ledge is where a floor in the
        // hand band [hand_z - 60, hand_z + 5] appears while
        // the character's own column has no such floor.
        let own = self.ground_height(pos[0], pos[1], hand_z + 5.0, 0.0);
        if let Some(g) = own {
            if g >= hand_z - 60.0 {
                return None; // already level with that floor
            }
        }
        let dl = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        if dl < 1e-6 {
            return None;
        }
        let d = [dir[0] / dl, dir[1] / dl];
        let mut prev = 10.0f32;
        let mut hit: Option<(f32, f32)> = None;
        let mut dist = 20.0f32;
        while dist <= 120.0 {
            let (x, y) = (pos[0] + d[0] * dist, pos[1] + d[1] * dist);
            if let Some(g) = self.ground_height(x, y, hand_z + 5.0, 0.0) {
                if g >= hand_z - 60.0 {
                    hit = Some((dist, g));
                    break;
                }
            }
            prev = dist;
            dist += 10.0;
        }
        let (mut dist, gz) = hit?;
        // Bisect between the last miss and the hit to tighten
        // the edge position.
        let mut lo = prev;
        let mut hi = dist;
        for _ in 0..4 {
            let mid = (lo + hi) / 2.0;
            let (x, y) = (pos[0] + d[0] * mid, pos[1] + d[1] * mid);
            match self.ground_height(x, y, hand_z + 5.0, 0.0) {
                Some(g) if g >= hand_z - 60.0 => hi = mid,
                _ => lo = mid,
            }
        }
        dist = hi;
        let top = [pos[0] + d[0] * dist, pos[1] + d[1] * dist, gz];
        // Normal: from the ledge point back toward the body.
        let mut n = [pos[0] - top[0], pos[1] - top[1]];
        let nl = (n[0] * n[0] + n[1] * n[1]).sqrt();
        if nl < 1e-6 {
            return None;
        }
        n = [n[0] / nl, n[1] / nl];
        Some(Ledge { top, normal: n })
    }

    /// Height of the highest walkable surface at (x, y) that is
    /// at or below `z_ref + step_up`. Walkable = the triangle's
    /// normal points up with cos(angle to +Z) >= 0.5 (slopes up
    /// to ~60 degrees). Returns None when no floor is found.
    pub fn ground_height(&self, x: f32, y: f32, z_ref: f32, step_up: f32) -> Option<f32> {
        let z_max = z_ref + step_up;
        let mut best: Option<f32> = None;
        for t in &self.tris {
            let (ax, ay, az) = (t[0], t[1], t[2]);
            let (bx, by, bz) = (t[3], t[4], t[5]);
            let (cx, cy, cz) = (t[6], t[7], t[8]);
            // Normal (z component decides walkability + facing).
            let ux = [bx - ax, by - ay, bz - az];
            let vx = [cx - ax, cy - ay, cz - az];
            let nz = ux[0] * vx[1] - ux[1] * vx[0];
            let nxy = ux[1] * vx[2] - ux[2] * vx[1];
            let nyx = ux[2] * vx[0] - ux[0] * vx[2];
            let nlen = (nz * nz + nxy * nxy + nyx * nyx).sqrt();
            if nlen < 1e-9 || nz / nlen < 0.5 {
                continue; // wall, ceiling, or degenerate
            }
            // Barycentric coordinates of (x, y) in the XY projection.
            let denom = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
            if denom.abs() < 1e-9 {
                continue;
            }
            let w0 = ((by - cy) * (x - cx) + (cx - bx) * (y - cy)) / denom;
            let w1 = ((cy - ay) * (x - cx) + (ax - cx) * (y - cy)) / denom;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let z = w0 * az + w1 * bz + w2 * cz;
            if z <= z_max && best.map(|b| z > b).unwrap_or(true) {
                best = Some(z);
            }
        }
        best
    }
}

/// True when the triangle is (mostly) vertical.
fn is_wall(t: &[f32; 9]) -> bool {
    let (ux, uy, uz) = (t[3] - t[0], t[4] - t[1], t[5] - t[2]);
    let (vx, vy, vz) = (t[6] - t[0], t[7] - t[1], t[8] - t[2]);
    let nz = ux * vy - uy * vx;
    let nl = ((uy * vz - uz * vy).powi(2) + (uz * vx - ux * vz).powi(2) + nz * nz).sqrt();
    nl >= 1e-9 && (nz / nl).abs() < 0.5
}

/// The 8 compass directions (k and k+4 are opposites).
fn dir8(k: usize) -> [f32; 2] {
    const D: [[f32; 2]; 8] = [
        [1.0, 0.0],
        [0.7071, 0.7071],
        [0.0, 1.0],
        [-0.7071, 0.7071],
        [-1.0, 0.0],
        [-0.7071, -0.7071],
        [0.0, -1.0],
        [0.7071, -0.7071],
    ];
    D[k % 8]
}

/// Cast a horizontal ray at height `z` from `c` along
/// `d`; nearest wall hit distance in (4, 75), if any.
fn ray_wall(local: &[&[f32; 9]], c: [f32; 2], z: f32, d: [f32; 2]) -> Option<f32> {
    let mut best: Option<f32> = None;
    for t in local {
        // The triangle must straddle the ray height.
        let (z0, z1, z2) = (t[2], t[5], t[8]);
        if z < z0.min(z1).min(z2) || z > z0.max(z1).max(z2) {
            continue;
        }
        let e1 = [t[3] - t[0], t[4] - t[1], t[5] - t[2]];
        let e2 = [t[6] - t[0], t[7] - t[1], t[8] - t[2]];
        // p = dir x e2 (dir has z = 0)
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
        // q = tv x e1
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
        if tt > 4.0 && tt < 75.0 && best.map_or(true, |b| tt < b) {
            best = Some(tt);
        }
    }
    best
}

/// Ray hits in all 8 directions at height `z`; Some only
/// when at least 5 directions hit (a surrounding ring).
fn ring_hits(local: &[&[f32; 9]], c: [f32; 2], z: f32) -> Option<[Option<f32>; 8]> {
    let mut hits: [Option<f32>; 8] = [None; 8];
    let mut n = 0;
    for k in 0..8 {
        hits[k] = ray_wall(local, c, z, dir8(k));
        if hits[k].is_some() {
            n += 1;
        }
    }
    if n >= 6 {
        Some(hits)
    } else {
        None
    }
}
