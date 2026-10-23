//! prince_set — the Prince's true animation set: resolve
//! AMIR_wow's descriptors archive-wide, then score every
//! resolved clip on his rig (frames, height, pose RMS).
//!
//! Usage: prince_set <pop3.bf>

use std::collections::HashMap;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn load_sector(bf: &BigFile, substr: &str) -> Option<Vec<u8>> {
    let e = bf.active_files().into_iter().find(|e| e.name.contains(substr))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

fn aabb_h(v: &[f32]) -> f32 {
    let mut mn = f32::INFINITY;
    let mut mx = f32::NEG_INFINITY;
    for p in v.chunks_exact(3) {
        mn = mn.min(p[2]);
        mx = mx.max(p[2]);
    }
    mx - mn
}

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: prince_set <bf>");
    let bf = BigFile::open(&bf_path).expect("open bf");
    // Global anim-family record index (first copy wins).
    let mut index: HashMap<u32, Vec<u8>> = HashMap::new();
    for e in bf.active_files() {
        if !e.name.contains("_wow_") {
            continue;
        }
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        for r in walk_wow(&data) {
            if (r.gro_type.unwrap_or(0) >> 16) == 0x0002 {
                index.entry(r.key).or_insert_with(|| data[r.data_range()].to_vec());
            }
        }
    }
    let rig_data = load_sector(&bf, "PrinceFinal_Shape").expect("rig sector");
    let ch = Character::load(&rig_data, 0x510022d3, "B_Pr").expect("character");
    let desc_sector = std::env::args().nth(2).unwrap_or_else(|| "AMIR_wow".to_string());
    let amir_data = load_sector(&bf, &desc_sector).expect("descriptor sector");
    let mut seen = std::collections::HashSet::new();
    for (i, r) in walk_wow(&amir_data).iter().enumerate() {
        if r.size != 52 {
            continue;
        }
        let Some(gt) = r.gro_type else { continue };
        if gt & 0xFFFF != 0xFF01 {
            continue;
        }
        let d = &amir_data[r.data_range()];
        let hi = u32::from_le_bytes(d[0..4].try_into().unwrap());
        let full = (hi << 16) | (gt >> 16);
        if !seen.insert(full) {
            continue;
        }
        let Some(body) = index.get(&full) else {
            println!("desc[{i}] {full:#010x}: unresolved");
            continue;
        };
        let clip = Clip::load(body);
        if clip.max_frame == 0 {
            println!("desc[{i}] {full:#010x}: 0 frames ({} B)", body.len());
            continue;
        }
        let f = clip.max_frame;
        let v0 = posed_verts(&ch, &clip, &ChannelMap::Identity, 0);
        let v1 = posed_verts(&ch, &clip, &ChannelMap::Identity, f / 2);
        let mut acc = 0f32;
        for k in 0..v0.len() {
            acc += (v1[k] - v0[k]).powi(2);
        }
        let rms = (acc / v0.len() as f32).sqrt();
        println!(
            "desc[{i}] {full:#010x}: {} frames, {} chans, height {:.2}, poseRMS {:.3} ({} B)",
            f + 1,
            clip.rot.len(),
            aabb_h(&v0),
            rms,
            body.len()
        );
    }
}
