//! Jade Game Object (GAO) parser — port of the read path of
//! Jade-Toolkit's Gao.cpp. Input is the record payload with the ".gao"
//! tag already stripped (i.e. WowRecord::data_range).
//!
//! Layout: header (version, editor_flags, identity, name_size) then
//! the name at offset 16; at 16 + name_size + 10 sits the 68-byte
//! global matrix (16 floats, column-major, + type u32); then a
//! bounding-volume block (48 B with the OBBox identity bit, else 24);
//! then identity-gated blocks: visual (gro_key, grm_key), hierarchy
//! (father_key + 68 B local matrix), gizmo pointers.

pub const ID_VISUAL: u32 = 0x0000_4000;
pub const ID_OBBOX: u32 = 0x0008_0000;
pub const ID_ADDMATRIX: u32 = 0x0020_0000;
pub const ID_HIERARCHY: u32 = 0x0040_0000;
pub const ID_BIT24: u32 = 0x0100_0000;

#[derive(Debug, Clone, Default)]
pub struct GaoInfo {
    pub version: u32,
    pub identity: u32,
    pub name: String,
    /// 16 floats, column-major (translation at [12],[13],[14]).
    pub global_matrix: Option<[f32; 16]>,
    pub gmat_type: u32,
    pub gro_key: u32,
    pub grm_key: u32,
    pub has_visual: bool,
    pub father_key: u32,
    pub local_matrix: Option<[f32; 16]>,
    pub has_hierarchy: bool,
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

fn read_mat(d: &[u8], off: usize) -> [f32; 16] {
    let mut m = [0f32; 16];
    for (i, v) in m.iter_mut().enumerate() {
        *v = lef(d, off + i * 4);
    }
    m
}

pub fn parse_gao(d: &[u8]) -> Option<GaoInfo> {
    if d.len() < 16 {
        return None;
    }
    let mut g = GaoInfo {
        version: le32(d, 0),
        identity: le32(d, 8),
        ..Default::default()
    };
    let ident = g.identity;
    let name_size = le32(d, 12) as usize;

    // Name: chars from 16 up to first NUL (toolkit accepts 16<end<136).
    let mut end = 16;
    while end < d.len() && d[end] != 0 {
        end += 1;
    }
    if end < d.len() && end > 16 && end < 16 + 120 {
        g.name = String::from_utf8_lossy(&d[16..end]).into_owned();
    }

    let mat_off = 16 + name_size + 10;
    if mat_off + 68 <= d.len() {
        g.global_matrix = Some(read_mat(d, mat_off));
        g.gmat_type = le32(d, mat_off + 64);
    }
    let bv_size = if ident & ID_OBBOX != 0 { 48 } else { 24 };
    let mut off = mat_off + 68 + bv_size;

    if ident & ID_VISUAL != 0 {
        g.has_visual = true;
        let mut vis_size = 42usize;
        if off + 16 <= d.len() {
            g.gro_key = le32(d, off);
            g.grm_key = le32(d, off + 4);
        }
        if off + 42 <= d.len() {
            let extra = le16(d, off + 38);
            if extra > 0 && off + 46 <= d.len() {
                let remaining = le32(d, off + 42) as usize;
                if remaining < d.len() {
                    vis_size = 46 + remaining;
                }
            }
        }
        off += vis_size;
    }

    if ident & ID_HIERARCHY != 0 {
        g.has_hierarchy = true;
        if off + 72 <= d.len() {
            g.father_key = le32(d, off);
            g.local_matrix = Some(read_mat(d, off + 4));
        } else {
            g.father_key = 0xFFFF_FFFF;
        }
        off += 72;
    }
    let _ = off;
    Some(g)
}

/// Apply a column-major 4x4 to a point.
/// General 4x4 inverse (Gauss-Jordan with pivoting).
/// Matrices here are column-major ([f32;16], translation
/// at [12..14]); the inversion treats them consistently,
/// so the result composes correctly with mat_mul.
pub fn mat_inv(m: &[f32; 16]) -> Option<[f32; 16]> {
    // Jade matrices often carry ~0 in m[15] (only the
    // 3x4 part is meaningful); complete the affine
    // bottom row before inverting.
    let mut m = *m;
    if m[15].abs() < 0.5 && m[3].abs() < 1e-6 && m[7].abs() < 1e-6 && m[11].abs() < 1e-6 {
        m[15] = 1.0;
    }
    let m = &m;
    let mut a = [[0.0f32; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            a[r][c] = m[c * 4 + r];
            a[r][c + 4] = if r == c { 1.0 } else { 0.0 };
        }
    }
    for col in 0..4 {
        let mut piv = col;
        for r in col + 1..4 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        if a[piv][col].abs() < 1e-9 {
            return None;
        }
        a.swap(col, piv);
        let d = a[col][col];
        for c in 0..8 {
            a[col][c] /= d;
        }
        for r in 0..4 {
            if r != col {
                let f = a[r][col];
                if f != 0.0 {
                    for c in 0..8 {
                        a[r][c] -= f * a[col][c];
                    }
                }
            }
        }
    }
    let mut out = [0.0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            out[c * 4 + r] = a[r][c + 4];
        }
    }
    Some(out)
}

pub fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// Multiply two column-major 4x4 matrices (a * b).
pub fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

pub const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
