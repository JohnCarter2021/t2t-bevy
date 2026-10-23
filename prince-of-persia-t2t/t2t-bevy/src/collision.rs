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
