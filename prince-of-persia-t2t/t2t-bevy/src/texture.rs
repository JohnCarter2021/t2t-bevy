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
