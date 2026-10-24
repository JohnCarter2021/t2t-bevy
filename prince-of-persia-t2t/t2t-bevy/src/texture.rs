//! Jade texture parser + decoders — port of Jade-Toolkit's Texture.cpp
//! (read path). Header is 44 bytes; pixel data starts at offset 52
//! (plus a 4-byte prefix for formats 1/5, 8-byte for format 11 v>=4).
//! Formats: 0 = BGRA, 1 = PAL8, 5 = DXT1, 6 = DXT3, 7 = DXT5, 11 = 4-bit
//! grey. DXT1/DXT5 use standard 565 bit-replication expansion and floor
//! interpolation; DXT3 uses the toolkit's legacy *255/N rounding.

pub const TEX_HEADER_SIZE: usize = 44;
pub const TEX_PIXDATA_OFF: usize = 52;
pub const TEX_MAGIC_1: u32 = 0xCAD0_1234;
pub const TEX_MAGIC_2: u32 = 0xC0DE_C0DE;

#[derive(Debug, Clone)]
pub struct TexInfo {
    pub width: u32,
    pub height: u32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub format: u32,
    pub version: u32,
    pub mip_count: u32,
    pub pix_start: usize,
}

fn le16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

pub fn is_texture_entry(d: &[u8]) -> bool {
    d.len() >= TEX_HEADER_SIZE + 8
        && le32(d, 0) == 0xFFFF_FFFF
        && le32(d, 20) == TEX_MAGIC_1
        && le32(d, 28) == TEX_MAGIC_2
}

pub fn parse_texture(d: &[u8]) -> Option<TexInfo> {
    if !is_texture_entry(d) {
        return None;
    }
    let log_w = le16(d, 8) as u32;
    let log_h = le16(d, 10) as u32;
    if log_w == 0 || log_h == 0 || log_w > 4096 || log_h > 4096 {
        return None;
    }
    let version = le32(d, 32);
    let fmt = le32(d, 36);
    let mip = le32(d, 48);
    if mip > 20 {
        return None;
    }
    let (mut w, mut h) = (log_w, log_h);
    if d.len() >= 48 {
        let aw = le32(d, 40);
        let ah = le32(d, 44);
        if aw > 0 && aw <= 4096 && ah > 0 && ah <= 4096 && (aw != log_w || ah != log_h) {
            w = aw;
            h = ah;
        }
    }
    let mut pix_start = TEX_PIXDATA_OFF;
    if fmt == 1 || fmt == 5 {
        pix_start = TEX_PIXDATA_OFF + 4;
    } else if fmt == 11 && version >= 4 {
        pix_start = TEX_PIXDATA_OFF + 8;
    }
    Some(TexInfo {
        width: w,
        height: h,
        logical_width: log_w,
        logical_height: log_h,
        format: fmt,
        version,
        mip_count: mip,
        pix_start,
    })
}

#[derive(Clone, Copy)]
struct Rgb {
    r: i32,
    g: i32,
    b: i32,
}

fn expand_std(c: u16) -> Rgb {
    let r = ((c >> 11) & 0x1F) as i32;
    let g = ((c >> 5) & 0x3F) as i32;
    let b = (c & 0x1F) as i32;
    Rgb {
        r: (r << 3) | (r >> 2),
        g: (g << 2) | (g >> 4),
        b: (b << 3) | (b >> 2),
    }
}
fn expand_legacy(c: u16) -> Rgb {
    let r = ((c >> 11) & 0x1F) as i32;
    let g = ((c >> 5) & 0x3F) as i32;
    let b = (c & 0x1F) as i32;
    Rgb {
        r: r * 255 / 31,
        g: g * 255 / 63,
        b: b * 255 / 31,
    }
}

fn bc_colors(c0: u16, c1: u16, legacy: bool) -> ([Rgb; 4], bool) {
    let a = if legacy { expand_legacy(c0) } else { expand_std(c0) };
    let b = if legacy { expand_legacy(c1) } else { expand_std(c1) };
    if c0 > c1 {
        (
            [
                a,
                b,
                Rgb { r: (2 * a.r + b.r) / 3, g: (2 * a.g + b.g) / 3, b: (2 * a.b + b.b) / 3 },
                Rgb { r: (a.r + 2 * b.r) / 3, g: (a.g + 2 * b.g) / 3, b: (a.b + 2 * b.b) / 3 },
            ],
            false,
        )
    } else {
        (
            [
                a,
                b,
                Rgb { r: (a.r + b.r) / 2, g: (a.g + b.g) / 2, b: (a.b + b.b) / 2 },
                Rgb { r: 0, g: 0, b: 0 },
            ],
            true,
        )
    }
}

fn put(out: &mut [u8], w: u32, x: u32, y: u32, c: Rgb, a: i32) {
    let o = (y as usize * w as usize + x as usize) * 4;
    out[o] = c.r as u8;
    out[o + 1] = c.g as u8;
    out[o + 2] = c.b as u8;
    out[o + 3] = a as u8;
}

/// Palette key for a PAL8 texture: the 4-byte prefix at offset 52,
/// read little-endian. The palette itself is a separate 1024-byte
/// record (256 BGRA entries) in the same sector, keyed by this value
/// (Jade-Toolkit `palette_for_texture`). Returns None for other
/// formats or short records.
pub fn palette_key(d: &[u8], ti: &TexInfo) -> Option<u32> {
    if ti.format != 1 || d.len() < 56 {
        return None;
    }
    Some(u32::from_le_bytes([d[52], d[53], d[54], d[55]]))
}

/// Decode a texture's base mip to RGBA8, resolving a PAL8 palette
/// when one is supplied. Palette entries are BGRA. Alpha rule
/// (measured, not assumed): if no index the texture actually uses
/// has alpha 255, the palette's 4th byte is not functioning as
/// alpha for this texture (Sand Garde / Human Guard palettes cap
/// at 191 — literal alpha would render a solid enemy as a ghost),
/// so alpha is forced to 255. When a 255 entry is used, alpha is
/// literal (Kaileena / Babylonian palettes: opaque charts over a
/// transparent background). Without a palette, PAL8 falls back to
/// greyscale, as before.
pub fn decode_texture_pal(d: &[u8], ti: &TexInfo, palette: Option<&[u8]>) -> Vec<u8> {
    if ti.format != 1 {
        return decode_texture(d, ti);
    }
    let Some(pal) = palette.filter(|p| p.len() >= 1024) else {
        return decode_texture(d, ti);
    };
    if ti.pix_start > d.len() {
        return Vec::new();
    }
    let pix = &d[ti.pix_start..];
    let (w, h) = (ti.width, ti.height);
    let need = w as usize * h as usize;
    if pix.len() < need {
        return Vec::new();
    }
    let mut used = [false; 256];
    for i in 0..need {
        used[pix[i] as usize] = true;
    }
    let max_alpha = (0..256)
        .filter(|i| used[*i])
        .map(|i| pal[i * 4 + 3])
        .max()
        .unwrap_or(0);
    let force_opaque = max_alpha < 255;
    let mut out = vec![0u8; need * 4];
    for i in 0..need {
        let e = &pal[pix[i] as usize * 4..pix[i] as usize * 4 + 4];
        out[4 * i] = e[2];
        out[4 * i + 1] = e[1];
        out[4 * i + 2] = e[0];
        out[4 * i + 3] = if force_opaque { 255 } else { e[3] };
    }
    out
}

/// Decode a texture's base mip to RGBA8 (w*h*4). `d` is the full
/// sub-entry data (header included).
pub fn decode_texture(d: &[u8], ti: &TexInfo) -> Vec<u8> {
    if ti.pix_start > d.len() {
        return Vec::new();
    }
    let pix = &d[ti.pix_start..];
    let (w, h) = (ti.width, ti.height);
    match ti.format {
        0 => {
            let need = w as usize * h as usize * 4;
            if pix.len() < need {
                return Vec::new();
            }
            let mut out = vec![0u8; need];
            for p in 0..w as usize * h as usize {
                out[4 * p] = pix[4 * p + 2];
                out[4 * p + 1] = pix[4 * p + 1];
                out[4 * p + 2] = pix[4 * p];
                out[4 * p + 3] = pix[4 * p + 3];
            }
            out
        }
        1 => {
            // PAL8 without a palette: greyscale fallback (toolkit parity).
            let need = w as usize * h as usize;
            if pix.len() < need {
                return Vec::new();
            }
            let mut out = vec![0u8; need * 4];
            for p in 0..need {
                out[4 * p] = pix[p];
                out[4 * p + 1] = pix[p];
                out[4 * p + 2] = pix[p];
                out[4 * p + 3] = 255;
            }
            out
        }
        5 => decode_dxt1(pix, w, h),
        6 => decode_dxt3(pix, w, h),
        7 => decode_dxt5(pix, w, h),
        11 => {
            let need = (w as usize * h as usize + 1) / 2;
            if pix.len() < need {
                return Vec::new();
            }
            let mut out = vec![0u8; w as usize * h as usize * 4];
            for y in 0..h {
                for x in 0..w {
                    let bi = (y as usize * w as usize + x as usize) / 2;
                    let v = if x % 2 == 0 {
                        (pix[bi] & 0x0F) * 17
                    } else {
                        ((pix[bi] >> 4) & 0x0F) * 17
                    };
                    put(&mut out, w, x, y, Rgb { r: v as i32, g: v as i32, b: v as i32 }, 255);
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

fn padded(pix: &[u8], need: usize) -> Vec<u8> {
    let mut buf = vec![0u8; need];
    let n = pix.len().min(need);
    buf[..n].copy_from_slice(&pix[..n]);
    buf
}

fn decode_dxt1(pix: &[u8], w: u32, h: u32) -> Vec<u8> {
    let bw = ((w + 3) / 4).max(1);
    let bh = ((h + 3) / 4).max(1);
    let buf = padded(pix, bw as usize * bh as usize * 8);
    let mut out = vec![0u8; w as usize * h as usize * 4];
    for by in 0..bh {
        for bx in 0..bw {
            let off = (by as usize * bw as usize + bx as usize) * 8;
            let c0 = le16(&buf, off);
            let c1 = le16(&buf, off + 2);
            let lut = le32(&buf, off + 4);
            let (col, is3) = bc_colors(c0, c1, false);
            for ty in 0..4u32 {
                for tx in 0..4u32 {
                    let (px, py) = (bx * 4 + tx, by * 4 + ty);
                    if px >= w || py >= h {
                        continue;
                    }
                    let idx = ((lut >> (2 * (4 * ty + tx))) & 3) as usize;
                    let a = if is3 && idx == 3 { 0 } else { 255 };
                    put(&mut out, w, px, py, col[idx], a);
                }
            }
        }
    }
    out
}

fn decode_dxt5(pix: &[u8], w: u32, h: u32) -> Vec<u8> {
    let bw = ((w + 3) / 4).max(1);
    let bh = ((h + 3) / 4).max(1);
    let buf = padded(pix, bw as usize * bh as usize * 16);
    let mut out = vec![0u8; w as usize * h as usize * 4];
    for by in 0..bh {
        for bx in 0..bw {
            let off = (by as usize * bw as usize + bx as usize) * 16;
            let (a0, a1) = (buf[off] as i32, buf[off + 1] as i32);
            let mut alut = [0i32; 8];
            alut[0] = a0;
            alut[1] = a1;
            if a0 > a1 {
                for i in 1..=6i32 {
                    alut[(i + 1) as usize] = ((7 - i) * a0 + i * a1) / 7;
                }
            } else {
                for i in 1..=4i32 {
                    alut[(i + 1) as usize] = ((5 - i) * a0 + i * a1) / 5;
                }
                alut[6] = 0;
                alut[7] = 255;
            }
            let mut abits: u64 = 0;
            for i in 0..6 {
                abits |= (buf[off + 2 + i] as u64) << (8 * i);
            }
            let c0 = le16(&buf, off + 8);
            let c1 = le16(&buf, off + 10);
            let clut = le32(&buf, off + 12);
            let (col, _) = bc_colors(c0, c1, false);
            for ty in 0..4u32 {
                for tx in 0..4u32 {
                    let (px, py) = (bx * 4 + tx, by * 4 + ty);
                    if px >= w || py >= h {
                        continue;
                    }
                    let p = 4 * ty + tx;
                    let alpha = alut[((abits >> (3 * p)) & 7) as usize];
                    let ci = ((clut >> (2 * p)) & 3) as usize;
                    put(&mut out, w, px, py, col[ci], alpha);
                }
            }
        }
    }
    out
}

fn decode_dxt3(pix: &[u8], w: u32, h: u32) -> Vec<u8> {
    let bw = (w / 4).max(1);
    let bh = (h / 4).max(1);
    let mut out = vec![0u8; w as usize * h as usize * 4];
    for by in 0..bh {
        for bx in 0..bw {
            let off = (by as usize * bw as usize + bx as usize) * 16;
            if off + 16 > pix.len() {
                return out;
            }
            let mut alpha = [0i32; 16];
            for row in 0..4 {
                let rv = le16(pix, off + row * 2);
                for coli in 0..4 {
                    alpha[row * 4 + coli] = (((rv >> (coli * 4)) & 0x0F) * 17) as i32;
                }
            }
            let c0 = le16(pix, off + 8);
            let c1 = le16(pix, off + 10);
            let clut = le32(pix, off + 12);
            let (col, _) = bc_colors(c0, c1, true);
            for ty in 0..4u32 {
                for tx in 0..4u32 {
                    let (px, py) = (bx * 4 + tx, by * 4 + ty);
                    if px >= w || py >= h {
                        continue;
                    }
                    let p = (4 * ty + tx) as usize;
                    let ci = ((clut >> (2 * (4 * ty + tx))) & 3) as usize;
                    put(&mut out, w, px, py, col[ci], alpha[p]);
                }
            }
        }
    }
    out
}

/// CPU mip chain for one decoded RGBA8 texture, in wgpu's
/// expected layout: level 0 (w x h), then each level halved
/// (max(1, w/2), max(1, h/2)) down to 1x1, levels concatenated
/// with no padding. Each level pixel is the 2x2 box average
/// of the level below (edge-clamped, so odd sizes work);
/// averaging is done in sRGB space, which is the standard
/// cheap approximation and visually fine for game art.
/// Returns (concatenated bytes, level count).
/// Precondition: rgba.len() == w * h * 4, w >= 1, h >= 1
/// (true for every decoder output and atlas in this crate).
pub fn mip_chain(rgba: &[u8], w: u32, h: u32) -> (Vec<u8>, u32) {
    debug_assert_eq!(rgba.len(), (w * h * 4) as usize);
    debug_assert!(w >= 1 && h >= 1);
    let mut out = Vec::with_capacity(((w * h * 4) as usize * 4) / 3 + 16);
    out.extend_from_slice(rgba);
    let mut level = rgba.to_vec();
    let (mut cw, mut ch) = (w, h);
    let mut count = 1u32;
    while cw > 1 || ch > 1 {
        let (nw, nh) = (cw.max(2) / 2, ch.max(2) / 2);
        // max(2)/2 gives max(1, w/2) for w >= 1.
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let sx = (x * 2).min(cw - 1);
                let sy = (y * 2).min(ch - 1);
                let sx2 = (x * 2 + 1).min(cw - 1);
                let sy2 = (y * 2 + 1).min(ch - 1);
                let o = (y * nw + x) * 4;
                for c in 0..4 {
                    let s = |px: u32, py: u32| level[((py * cw + px) * 4 + c) as usize] as u32;
                    next[(o + c) as usize] =
                        ((s(sx, sy) + s(sx2, sy) + s(sx, sy2) + s(sx2, sy2) + 2) / 4) as u8;
                }
            }
        }
        out.extend_from_slice(&next);
        level = next;
        (cw, ch) = (nw, nh);
        count += 1;
    }
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal8_ti() -> TexInfo {
        TexInfo {
            width: 2,
            height: 2,
            logical_width: 2,
            logical_height: 2,
            format: 1,
            version: 0,
            mip_count: 0,
            pix_start: 56,
        }
    }

    fn pal8_data(pixels: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; 56];
        d.extend_from_slice(pixels);
        d
    }

    #[test]
    fn pal8_bgra_swap_and_literal_alpha() {
        // Palette entries are BGRA; one used entry is fully opaque,
        // so alpha stays literal.
        let mut pal = vec![0u8; 1024];
        pal[0..4].copy_from_slice(&[10, 20, 30, 255]); // idx 0
        pal[4..8].copy_from_slice(&[40, 50, 60, 0]); // idx 1
        let d = pal8_data(&[0, 1, 0, 1]);
        let out = decode_texture_pal(&d, &pal8_ti(), Some(&pal));
        assert_eq!(out, vec![30, 20, 10, 255, 60, 50, 40, 0, 30, 20, 10, 255, 60, 50, 40, 0]);
    }

    #[test]
    fn pal8_alpha_forced_when_no_opaque_entry_used() {
        // All used entries cap below 255: the 4th byte is not
        // functioning as alpha — force opaque (Sand Garde case).
        let mut pal = vec![0u8; 1024];
        pal[0..4].copy_from_slice(&[10, 20, 30, 191]);
        pal[4..8].copy_from_slice(&[40, 50, 60, 100]);
        let d = pal8_data(&[0, 1, 0, 1]);
        let out = decode_texture_pal(&d, &pal8_ti(), Some(&pal));
        assert_eq!(out, vec![30, 20, 10, 255, 60, 50, 40, 255, 30, 20, 10, 255, 60, 50, 40, 255]);
    }

    #[test]
    fn pal8_no_palette_is_greyscale() {
        let d = pal8_data(&[7, 200, 7, 200]);
        let out = decode_texture_pal(&d, &pal8_ti(), None);
        assert_eq!(out, vec![7, 7, 7, 255, 200, 200, 200, 255, 7, 7, 7, 255, 200, 200, 200, 255]);
    }

    #[test]
    fn palette_key_reads_prefix() {
        let mut d = vec![0u8; 56];
        d[52..56].copy_from_slice(&0x51003bdau32.to_le_bytes());
        assert_eq!(palette_key(&d, &pal8_ti()), Some(0x51003bda));
        let dxt5 = TexInfo { format: 7, ..pal8_ti() };
        assert_eq!(palette_key(&d, &dxt5), None);
    }

    #[test]
    fn mip_chain_2x2_averages() {
        // 2x2 distinct reds -> 1x1 mean (rounded).
        let rgba = vec![
            10, 0, 0, 255, 20, 0, 0, 255, //
            30, 0, 0, 255, 42, 0, 0, 255,
        ];
        let (chain, count) = mip_chain(&rgba, 2, 2);
        assert_eq!(count, 2);
        assert_eq!(chain.len(), 16 + 4);
        assert_eq!(&chain[16..], &[26, 0, 0, 255]); // (10+20+30+42+2)/4 = 26
    }

    #[test]
    fn mip_chain_512_square_counts_and_size() {
        let rgba = vec![128u8; 512 * 512 * 4];
        let (chain, count) = mip_chain(&rgba, 512, 512);
        assert_eq!(count, 10);
        assert_eq!(chain.len(), 4 * (262144 + 65536 + 16384 + 4096 + 1024 + 256 + 64 + 16 + 4 + 1));
    }

    #[test]
    fn mip_chain_odd_and_atlas_dims() {
        // 3x5 -> 1x2 -> 1x1 (edge-clamped halving).
        let (_, count) = mip_chain(&vec![0u8; 3 * 5 * 4], 3, 5);
        assert_eq!(count, 3);
        // The Prince atlas shape 512x1088 -> 11 levels.
        let (_, count) = mip_chain(&vec![0u8; 512 * 1088 * 4], 512, 1088);
        assert_eq!(count, 11);
        // 1x1 stays a single level.
        let (_, count) = mip_chain(&vec![9u8; 4], 1, 1);
        assert_eq!(count, 1);
    }

    #[test]
    fn mip_chain_level0_is_verbatim() {
        let rgba: Vec<u8> = (0..4 * 4 * 4).map(|i| (i % 251) as u8).collect();
        let (chain, _) = mip_chain(&rgba, 4, 4);
        assert_eq!(&chain[..rgba.len()], rgba.as_slice());
    }
}
