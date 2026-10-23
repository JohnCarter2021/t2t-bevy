//! Jade material parser — port of the read path of Jade-Toolkit's
//! Material.cpp. By gro_type: 3 = single, 5 = multitexture, 4 = multi.
//! Single/multitexture hold colours + a texture key at offset 24
//! (multitexture: the real texture key sits at hdr_size + 4, accepted
//! when its top byte is 0x13/0x33). Multi holds a positional array of
//! sub-material keys at offset 8; GEO element matIds index it directly.

#[derive(Debug, Clone, Default)]
pub struct MatInfo {
    pub kind: u8, // 0 single, 1 multitexture, 2 multi
    pub ambient: u32,
    pub diffuse: u32,
    pub specular: u32,
    pub texture_keys: Vec<u32>,
    pub sub_material_keys: Vec<u32>,
}

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

pub fn parse_material(d: &[u8], gro_type: u32) -> Option<MatInfo> {
    match gro_type {
        3 => parse_single(d),
        5 => parse_multitex(d),
        4 => parse_multi(d),
        _ => None,
    }
}

fn valid_key(k: u32) -> bool {
    k != 0 && k != 0xFFFF_FFFF
}

fn parse_single(d: &[u8]) -> Option<MatInfo> {
    if d.len() < 32 {
        return None;
    }
    let mut m = MatInfo {
        kind: 0,
        ambient: le32(d, 0),
        diffuse: le32(d, 4),
        specular: le32(d, 8),
        ..Default::default()
    };
    let tk = le32(d, 24);
    if valid_key(tk) {
        m.texture_keys.push(tk);
    }
    Some(m)
}

fn parse_multitex(d: &[u8]) -> Option<MatInfo> {
    if d.len() < 32 {
        return None;
    }
    let sexpraw = le32(d, 12);
    let hdr_size = if sexpraw & 0x8000_0000 != 0 { 36 } else { 32 };
    let mut m = MatInfo {
        kind: 1,
        ambient: le32(d, 0),
        diffuse: le32(d, 4),
        specular: le32(d, 8),
        ..Default::default()
    };
    let off = hdr_size + 4;
    if off + 4 <= d.len() {
        let tk = le32(d, off);
        let hi = tk >> 24;
        if valid_key(tk) && (hi == 0x13 || hi == 0x33) {
            m.texture_keys.push(tk);
        }
    }
    Some(m)
}

fn parse_multi(d: &[u8]) -> Option<MatInfo> {
    if d.len() < 8 {
        return None;
    }
    let n_sub = le32(d, 4) as usize;
    let avail = (d.len() - 8) / 4;
    let n = n_sub.min(avail);
    let mut m = MatInfo {
        kind: 2,
        ..Default::default()
    };
    for i in 0..n {
        m.sub_material_keys.push(le32(d, 8 + i * 4));
    }
    Some(m)
}

/// Byte offset of the texture key inside a "leaf" material payload,
/// by its leading kind word (4..=9). Toolkit Material.cpp parity.
fn kind_tex_offset(kind: u32) -> Option<usize> {
    match kind {
        4 | 5 | 6 => Some(40),
        7 => Some(42),
        8 => Some(50),
        9 => Some(63),
        _ => None,
    }
}

/// The texture key a leaf material points at, or None. This is the
/// reader the toolkit's own exporters use on sub-material payloads.
pub fn resolve_texture_key(d: &[u8]) -> Option<u32> {
    if d.len() < 4 {
        return None;
    }
    let off = kind_tex_offset(le32(d, 0))?;
    if off + 4 > d.len() {
        return None;
    }
    let key = le32(d, off);
    if key == 0 || key == 0xFFFF_FFFF {
        None
    } else {
        Some(key)
    }
}

fn material_texture_key(gro: u32, data: &[u8]) -> Option<u32> {
    if let Some(m) = parse_material(data, gro) {
        if let Some(k) = m.texture_keys.first() {
            return Some(*k);
        }
    }
    resolve_texture_key(data)
}

/// Resolve a material record (by gro_type) to the texture key for
/// element `mat_id`: multi indexes sub-materials positionally, then
/// the sub-material's own texture key. `by_key` resolves records.
pub fn texture_key_for(
    grm_key: u32,
    mat_id: u32,
    by_key: &std::collections::HashMap<u32, (u32, &[u8])>,
) -> Option<u32> {
    let &(gro, data) = by_key.get(&grm_key)?;
    let mat = parse_material(data, gro)?;
    match mat.kind {
        2 => {
            let sub_key = *mat.sub_material_keys.get(mat_id as usize)?;
            let &(sgro, sdata) = by_key.get(&sub_key)?;
            material_texture_key(sgro, sdata)
        }
        _ => material_texture_key(gro, data),
    }
}
