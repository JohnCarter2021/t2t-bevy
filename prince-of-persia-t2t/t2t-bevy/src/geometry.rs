//! Jade GEO geometry parser (PC, versions 7/8) — port of the read path
//! of Jade-Toolkit's Geometry.cpp, including the MRM/skin block.
//!
//! Layout after the 10-u32 header (version, flags1, flags2, nb_pts,
//! abs_count, has_abs, nb_uvs, nb_elems, mrm_marker, skin_ok3):
//! when mrm_marker == 0xC0DE2002 a skin block follows the header at
//! offset 36 (flags u16, list count u16; per list: bone index u16,
//! vertex count u16, 16-float bind matrix, matrix type u32, then
//! (vertex u16, weight u16) pairs) and a fresh skin_ok3 word after it
//! gates the normals array. Then: vertices (nb_pts * 3 f32), normals
//! (same, only if skin_ok3 != 0), colours (min(abs_count, nb_pts) ARGB
//! u32, only if has_abs != 0), uvs (nb_uvs * 2 f32), element headers
//! (nb_elems * (nTri u32, matId u32)), then triangles (16 bytes each:
//! 3 vertex u16, 3 uv u16, 4 bytes pad/other). Triangles with an
//! index >= nb_pts are dropped, toolkit-style.

/// One bone's skin list: the vertices it drives, with raw weights.
#[derive(Debug, Clone, Default)]
pub struct SkinBone {
    pub bone_idx: u16,
    pub bind_matrix: [f32; 16],
    pub matrix_type: u32,
    /// (vertex index, raw weight) — Jade stores weights as u16
    /// fractions; per-vertex sums are checked by callers.
    pub weights: Vec<(u16, u16)>,
}

#[derive(Debug, Clone, Default)]
pub struct GeoInfo {
    pub version: u32,
    pub vertices: Vec<f32>, // 3 * nb_points
    pub normals: Vec<f32>,  // 3 * nb_points or empty
    pub uvs: Vec<f32>,      // 2 * nb_uvs or empty
    pub colors: Vec<u8>,    // 4 * ncol RGBA or empty
    pub elements: Vec<(u32, u32)>, // (nTri, matId)
    pub faces: Vec<[u16; 7]>,      // i0,i1,i2, uv0,uv1,uv2, elem
    /// Skin block when the MRM marker is present: (flags, bones).
    pub skin: Option<(u16, Vec<SkinBone>)>,
}

impl GeoInfo {
    pub fn nb_points(&self) -> usize {
        self.vertices.len() / 3
    }
    pub fn nb_tris(&self) -> usize {
        self.faces.len()
    }
    /// Axis-aligned bounds ((min), (max)) over all vertices.
    pub fn aabb(&self) -> ([f32; 3], [f32; 3]) {
        let mut mn = [f32::INFINITY; 3];
        let mut mx = [f32::NEG_INFINITY; 3];
        for v in self.vertices.chunks_exact(3) {
            for i in 0..3 {
                mn[i] = mn[i].min(v[i]);
                mx[i] = mx[i].max(v[i]);
            }
        }
        (mn, mx)
    }
}

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn le16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn lef(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

pub const MRM_MARKER: u32 = 0xC0DE_2002;

pub fn is_geometry_entry(d: &[u8], gro_type: Option<u32>) -> bool {
    if let Some(g) = gro_type {
        if g != 1 {
            return false;
        }
    }
    if d.len() < 40 {
        return false;
    }
    let version = le32(d, 0);
    if version != 7 && version != 8 {
        return false;
    }
    let nb_pts = le32(d, 12);
    if nb_pts < 1 || nb_pts > 100_000 {
        return false;
    }
    le32(d, 28) <= 10_000
}

/// Why a geometry-looking payload did not parse.
#[derive(Debug, PartialEq)]
pub enum GeoError {
    NotGeometry,
    Truncated,
}

/// Parse the skin block at offset 36 (toolkit parse_skin_data parity).
/// Returns (flags, bones, bytes consumed from offset 36).
fn parse_skin_data(d: &[u8]) -> Option<(u16, Vec<SkinBone>, usize)> {
    let n = d.len();
    let start = 36usize;
    if start + 4 > n {
        return None;
    }
    let flags = le16(d, start);
    let num_lists = le16(d, start + 2) as usize;
    let mut off = start + 4;
    let mut bones = Vec::new();
    for _ in 0..num_lists {
        if off + 72 > n {
            return None;
        }
        let bone_idx = le16(d, off);
        let num_verts = le16(d, off + 2) as usize;
        off += 4;
        let mut bind = [0f32; 16];
        for (k, v) in bind.iter_mut().enumerate() {
            *v = lef(d, off + k * 4);
        }
        off += 64;
        let matrix_type = le32(d, off);
        off += 4;
        if off + num_verts * 4 > n {
            return None;
        }
        let mut weights = Vec::with_capacity(num_verts);
        for wi in 0..num_verts {
            let wo = off + wi * 4;
            weights.push((le16(d, wo), le16(d, wo + 2)));
        }
        off += num_verts * 4;
        bones.push(SkinBone {
            bone_idx,
            bind_matrix: bind,
            matrix_type,
            weights,
        });
    }
    Some((flags, bones, off - start))
}

pub fn parse_geometry(d: &[u8]) -> Result<GeoInfo, GeoError> {
    if !is_geometry_entry(d, None) {
        return Err(GeoError::NotGeometry);
    }
    let mut g = GeoInfo {
        version: le32(d, 0),
        ..Default::default()
    };
    let nb_pts = le32(d, 12) as usize;
    let abs_count = le32(d, 16) as usize;
    let has_abs = le32(d, 20);
    let nb_uvs = le32(d, 24) as usize;
    let nb_elems = le32(d, 28) as usize;
    let mrm_marker = le32(d, 32);
    let mut skin_ok3 = le32(d, 36);
    let n = d.len();
    let mut off = 40usize;

    if mrm_marker == MRM_MARKER {
        let (flags, bones, consumed) = parse_skin_data(d).ok_or(GeoError::Truncated)?;
        if consumed == 0 {
            return Err(GeoError::Truncated);
        }
        let skin_off = 36 + consumed;
        if skin_off + 4 > n {
            return Err(GeoError::Truncated);
        }
        // The word after the skin block replaces skin_ok3 for gating
        // normals (the header's offset-36 word held the skin flags).
        skin_ok3 = le32(d, skin_off);
        g.skin = Some((flags, bones));
        off = skin_off + 4;
    }

    // Vertices
    if off + nb_pts * 12 > n {
        return Err(GeoError::Truncated);
    }
    g.vertices.reserve(nb_pts * 3);
    for i in 0..nb_pts {
        let o = off + i * 12;
        g.vertices.push(lef(d, o));
        g.vertices.push(lef(d, o + 4));
        g.vertices.push(lef(d, o + 8));
    }
    off += nb_pts * 12;

    // Normals
    if skin_ok3 != 0 {
        if off + nb_pts * 12 > n {
            return Err(GeoError::Truncated);
        }
        g.normals.reserve(nb_pts * 3);
        for i in 0..nb_pts {
            let o = off + i * 12;
            g.normals.push(lef(d, o));
            g.normals.push(lef(d, o + 4));
            g.normals.push(lef(d, o + 8));
        }
        off += nb_pts * 12;
    }

    // Colours
    if has_abs != 0 {
        let n_col = abs_count.min(nb_pts);
        if off + n_col * 4 <= n {
            g.colors.reserve(n_col * 4);
            for i in 0..n_col {
                let argb = le32(d, off + i * 4);
                g.colors.push(((argb >> 16) & 0xFF) as u8);
                g.colors.push(((argb >> 8) & 0xFF) as u8);
                g.colors.push((argb & 0xFF) as u8);
                g.colors.push(((argb >> 24) & 0xFF) as u8);
            }
        }
        off += n_col * 4;
    }

    // UVs
    if nb_uvs > 0 && off + nb_uvs * 8 <= n {
        g.uvs.reserve(nb_uvs * 2);
        for i in 0..nb_uvs {
            let o = off + i * 8;
            g.uvs.push(lef(d, o));
            g.uvs.push(lef(d, o + 4));
        }
    }
    off += nb_uvs * 8;

    // Element headers
    if off + nb_elems * 8 > n {
        return Err(GeoError::Truncated);
    }
    let mut total_tris = 0usize;
    for i in 0..nb_elems {
        let n_tri = le32(d, off + i * 8);
        let mat = le32(d, off + i * 8 + 4);
        g.elements.push((n_tri, mat));
        total_tris += n_tri as usize;
    }
    off += nb_elems * 8;

    if total_tris == 0 || nb_elems == 0 {
        return Ok(g); // header-only mesh
    }
    if off + total_tris * 16 > n {
        return Err(GeoError::Truncated);
    }
    let mut tri_idx = 0usize;
    for (ei, &(elem_tris, _)) in g.elements.clone().iter().enumerate() {
        for _ in 0..elem_tris {
            let toff = off + tri_idx * 16;
            let (i0, i1, i2) = (le16(d, toff), le16(d, toff + 2), le16(d, toff + 4));
            let (u0, u1, u2) = (le16(d, toff + 6), le16(d, toff + 8), le16(d, toff + 10));
            if (i0 as usize) < nb_pts && (i1 as usize) < nb_pts && (i2 as usize) < nb_pts {
                g.faces.push([i0, i1, i2, u0, u1, u2, ei as u16]);
            }
            tri_idx += 1;
        }
    }
    Ok(g)
}
