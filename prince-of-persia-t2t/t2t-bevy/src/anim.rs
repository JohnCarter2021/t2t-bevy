//! Jade animation records — first decoded family (2026-10-22).
//!
//! Records with gro_type >> 16 == 0x0002 are animation data. The
//! small records (e.g. gro 0x20006 / 0x20036 in 100_Quai) are arrays
//! of fixed 72-byte units — one unit per animated bone:
//!
//!   unit (72 B) = block A (28 B) + block B (44 B)
//!   A: [0]=0x03, [1]=0xFA (0xFE on the first unit), [2..4]=bone
//!      index u16 (0xFFFF on the first unit), [4..8]=16, [8..12]=2,
//!      [12..]=flags/bitfield, then a 4-byte value at [19..23],
//!      a zero separator byte, and the SAME 4 bytes at [24..28].
//!   B: same 16-byte header shape with [4..8]=32, then the bone's
//!      translation as 3 x f32 at [47..59], a zero separator byte,
//!      and the same 12 bytes repeated at [60..72].
//!
//! The A-block's 4-byte value is a smallest-three compressed
//! quaternion (decoded 2026-10-22): the top 2 bits index the
//! largest component; the remaining three components follow as
//! 10-bit fields, MSB-first, each mapped from [0, 1023] onto
//! [-1/sqrt(2), +1/sqrt(2)]; the largest component is rebuilt as
//! sqrt(1 - sum of squares) and is non-negative. Verified on 179
//! harvested units (zero invariant violations; several units match
//! known bind-pose bone rotations to < 0.002 quat distance).
//! The B-block triple decodes cleanly as f32s and shows the
//! expected left/right mirrored pairs of a humanoid rig; its exact
//! frame semantics (local translation keys vs another channel)
//! are still being pinned down.
//!
//! The large gro 0x2002b clip records share the block headers but
//! carry long key runs; their segmentation is future work.

/// One bone's entry in a 72-byte-unit animation record.
#[derive(Debug, Clone, PartialEq)]
pub struct PoseUnit {
    /// Bone index within the record's track list; the first unit
    /// carries 0xFFFF and zero translation (a header/root unit).
    pub index: u16,
    /// The A-block's compressed rotation value, raw.
    pub rotation_raw: u32,
    /// The A-block's rotation decoded as a quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// The B-block's translation key (metres, Jade axes).
    pub translation: [f32; 3],
}

/// Decode a smallest-three compressed quaternion: top 2 bits are
/// the index of the largest component, then three 10-bit fields
/// (MSB-first) mapped onto [-1/sqrt(2), +1/sqrt(2)]; the largest
/// component is sqrt(1 - sum of squares of the stored three).
pub fn decode_rotation(raw: u32) -> [f32; 4] {
    let idx = (raw >> 30) as usize;
    let r = std::f32::consts::FRAC_1_SQRT_2;
    let mut stored = [0.0f32; 3];
    for (i, s) in stored.iter_mut().enumerate() {
        let v = (raw >> (20 - 10 * i)) & 0x3FF;
        *s = (v as f32 / 1023.0 * 2.0 - 1.0) * r;
    }
    let sum = stored.iter().map(|c| c * c).sum::<f32>();
    let big = (1.0 - sum).max(0.0).sqrt();
    let mut q = [0.0f32; 4];
    q[idx] = big;
    let mut j = 0;
    for (i, c) in q.iter_mut().enumerate() {
        if i != idx {
            *c = stored[j];
            j += 1;
        }
    }
    q
}

fn le16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn lef(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

pub const UNIT_SIZE: usize = 72;

/// True when `d` has the shape of a 72-byte-unit animation record:
/// length is a multiple of 72 and every unit carries the block
/// markers (0x03 lead, size words 16/32, channel word 2, zero
/// separators, duplicated A value and B payload).
pub fn is_pose_record(d: &[u8]) -> bool {
    if d.is_empty() || !d.len().is_multiple_of(UNIT_SIZE) {
        return false;
    }
    d.chunks_exact(UNIT_SIZE).all(|u| {
        u[0] == 3
            && le32(u, 4) == 16
            && le32(u, 8) == 2
            && le32(u, 28 + 4) == 32
            && le32(u, 28 + 8) == 2
            && u[23] == 0
            && u[59] == 0
            && u[19..23] == u[24..28]
            && u[47..59] == u[60..72]
    })
}

/// Parse a 72-byte-unit animation record into per-bone pose units.
pub fn parse_pose_record(d: &[u8]) -> Option<Vec<PoseUnit>> {
    if !is_pose_record(d) {
        return None;
    }
    Some(
        d.chunks_exact(UNIT_SIZE)
            .map(|u| {
                let raw = le32(u, 19);
                PoseUnit {
                    index: le16(u, 2),
                    rotation_raw: raw,
                    rotation: decode_rotation(raw),
                    translation: [lef(u, 47), lef(u, 51), lef(u, 55)],
                }
            })
            .collect(),
    )
}


/// A section header inside a 0x2002b clip record: a lead word
/// (0x03, variant byte, 0xff/0x00, 0xff/0x00), a size word and a
/// count word. Two shapes verified across five clips:
/// "directory" sections have size == count * 16; "stream"
/// sections have size == count * 8 and are followed by an 18-byte
/// header and `count` 6-byte keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Directory,
    Stream,
}

#[derive(Debug, Clone, Copy)]
pub struct ClipSection {
    pub offset: usize,
    pub size_word: u32,
    pub count: u32,
    pub kind: SectionKind,
}

/// Find clip section headers in a 0x2002b record's data.
pub fn find_sections(d: &[u8]) -> Vec<ClipSection> {
    let mut out = Vec::new();
    if d.len() < 12 {
        return out;
    }
    for i in 0..d.len() - 12 {
        // Lead word: [0x03 or 0x00][0xe0..=0xff][channel or 0xff]
        // [0x00 or 0xff]. The size/count identities below are the
        // real filter.
        if !(d[i] == 0x03 || d[i] == 0x00)
            || !(0xe0..=0xff).contains(&d[i + 1])
            || !(d[i + 2] == 0xff && d[i + 3] == 0xff || d[i + 3] == 0)
        {
            continue;
        }
        let (size, count) = (le32(d, i + 4), le32(d, i + 8));
        let kind = if size == count * 16 {
            Some(SectionKind::Directory)
        } else if count > 0 && size == count * 8 {
            Some(SectionKind::Stream)
        } else {
            None
        };
        if let Some(kind) = kind {
            out.push(ClipSection { offset: i, size_word: size, count, kind });
        }
    }
    out
}

/// One packed animation key: a frame delta and a smallest-three
/// compressed quaternion (rotation stream layout).
#[derive(Debug, Clone, Copy)]
pub struct RotKey {
    pub delta: u16,
    pub rotation: [f32; 4],
}

/// Read `count` 6-byte rotation keys starting at `start`.
pub fn read_rot_keys(d: &[u8], start: usize, count: usize) -> Option<Vec<RotKey>> {
    let end = start.checked_add(count.checked_mul(6)?)?;
    if end > d.len() {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = start + i * 6;
        out.push(RotKey {
            delta: le16(d, o),
            rotation: decode_rotation(le32(d, o + 2)),
        });
    }
    Some(out)
}


/// Read `count` 5-byte dense rotation keys starting at
/// `start`: [u8 delta][u32 smallest-three quat].
pub fn read_rot5_keys(d: &[u8], start: usize, count: usize) -> Option<Vec<RotKey>> {
    let end = start.checked_add(count.checked_mul(5)?)?;
    if end > d.len() {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = start + i * 5;
        out.push(RotKey {
            delta: d[o] as u16,
            rotation: decode_rotation(le32(d, o + 1)),
        });
    }
    Some(out)
}

/// Read `count` 13-byte dense translation keys starting at
/// `start`: [u8 delta][f32 x 3].
pub fn read_trans13_keys(d: &[u8], start: usize, count: usize) -> Option<Vec<TransKey>> {
    let end = start.checked_add(count.checked_mul(13)?)?;
    if end > d.len() {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = start + i * 13;
        out.push(TransKey {
            delta: d[o] as u16,
            value: [lef(d, o + 1), lef(d, o + 5), lef(d, o + 9)],
        });
    }
    Some(out)
}

/// One packed translation key: a frame delta and a full f32
/// vec3 (translation stream layout, 14 bytes per key).
#[derive(Debug, Clone, Copy)]
pub struct TransKey {
    pub delta: u16,
    pub value: [f32; 3],
}

/// Read `count` 14-byte translation keys starting at `start`.
pub fn read_trans_keys(d: &[u8], start: usize, count: usize) -> Option<Vec<TransKey>> {
    let end = start.checked_add(count.checked_mul(14)?)?;
    if end > d.len() {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = start + i * 14;
        out.push(TransKey {
            delta: le16(d, o),
            value: [lef(d, o + 2), lef(d, o + 6), lef(d, o + 10)],
        });
    }
    Some(out)
}

/// Role of a clip section, resolved from its span between
/// consecutive section leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionRole {
    /// First section: the root track's rotation stream.
    Root,
    /// The track directory (16 B header + count rows of 16 B).
    Directory,
    /// Rotation stream: 18 B header + count 6-byte keys.
    Rot,
    /// Rotation stream, dense variant: 18 B header + count
    /// 5-byte keys ([u8 delta][u32 smallest-three quat]).
    Rot5,
    /// Translation stream: 18 B header + count 14-byte keys.
    Trans,
    /// Translation stream, dense variant: 18 B header + count
    /// 13-byte keys ([u8 delta][f32 x 3]).
    Trans13,
    /// Trailing cinematic footer (placement vec3s + event
    /// data); only as the record's last section.
    Footer,
    /// Lead whose span matches no known layout.
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub struct ClipSectionInfo {
    pub offset: usize,
    pub span: usize,
    pub chan: usize,
    pub count: u32,
    pub role: SectionRole,
}

/// Walk a whole 0x2002b clip record: find the section leads,
/// derive spans from consecutive offsets, and classify each
/// section by which layout its span satisfies exactly.
pub fn parse_clip(d: &[u8]) -> Vec<ClipSectionInfo> {
    let secs = find_sections(d);
    let mut out = Vec::with_capacity(secs.len());
    for (i, s) in secs.iter().enumerate() {
        let end = secs.get(i + 1).map(|n| n.offset).unwrap_or(d.len());
        let span = end - s.offset;
        let count = s.count as usize;
        let role = if i == 0 {
            SectionRole::Root
        } else if span == 16 + count * 16 {
            SectionRole::Directory
        } else if span == 18 + count * 6 && s.kind == SectionKind::Stream {
            SectionRole::Rot
        } else if span == 18 + count * 5 && s.kind == SectionKind::Stream {
            SectionRole::Rot5
        } else if span == 18 + count * 14 && s.kind == SectionKind::Directory {
            SectionRole::Trans
        } else if span == 18 + count * 13 && s.kind == SectionKind::Directory {
            SectionRole::Trans13
        } else if i + 1 == secs.len() {
            SectionRole::Footer
        } else {
            SectionRole::Unknown
        };
        out.push(ClipSectionInfo {
            offset: s.offset,
            span,
            chan: if s.offset + 2 < d.len() { d[s.offset + 2] as usize } else { 0 },
            count: s.count,
            role,
        });
    }
    out
}
