//! prince_keys — score specific clip keys (from a text
//! file, one 0x… key per line) on the Prince's rig,
//! wherever in the archive each record lives.
//!
//! Usage: prince_keys <pop3.bf> <keys-file>

use std::collections::HashMap;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn load_sector(bf: &BigFile, substr: &str) -> Option<Vec<u8>> {
    let e = bf.active_files().into_iter().find(|e| e.name.contains(substr))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage: prince_keys <bf> <keys-file>");
    let keys_path = std::env::args().nth(2).expect("usage");
    let want: Vec<u32> = std::fs::read_to_string(&keys_path)
        .expect("read keys")
        .lines()
        .filter_map(|l| u32::from_str_radix(l.trim().trim_start_matches("0x"), 16).ok())
        .collect();
    let bf = BigFile::open(&bf_path).expect("open bf");
    let mut found: HashMap<u32, (String, Vec<u8>)> = HashMap::new();
    for e in bf.active_files() {
        if !e.name.contains("_wow_") {
            continue;
        }
        let Ok(data) = decompress_entry(bf.read_data(e.index)) else {
            continue;
        };
        for r in walk_wow(&data) {
            if want.contains(&r.key) && !found.contains_key(&r.key) {
                found.insert(r.key, (e.name.clone(), data[r.data_range()].to_vec()));
            }
        }
    }
    let rig_sector = std::env::var("T2T_RIG_SECTOR").unwrap_or_else(|_| "PrinceFinal_Shape".into());
    let rig_geo = std::env::var("T2T_RIG_GEO")
        .ok()
        .and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x510022d3);
    let rig_prefix = std::env::var("T2T_RIG_PREFIX").unwrap_or_else(|_| "B_Pr".into());
    let rig_data = load_sector(&bf, &rig_sector).expect("rig sector");
    let ch = Character::load(&rig_data, rig_geo, &rig_prefix).expect("character");
    for k in &want {
        let Some((sec, body)) = found.get(k) else {
            println!("{k:#010x}: NOT FOUND");
            continue;
        };
        let clip = Clip::load(body);
        if clip.max_frame == 0 {
            println!("{k:#010x}: 0 frames in {}", sec.split("_wow_").next().unwrap_or(sec));
            continue;
        }
        let f = clip.max_frame;
        let v0 = posed_verts(&ch, &clip, &ChannelMap::Identity, 0);
        let v1 = posed_verts(&ch, &clip, &ChannelMap::Identity, f / 2);
        let v2 = posed_verts(&ch, &clip, &ChannelMap::Identity, f);
        let (mut mn, mut mx) = (f32::INFINITY, f32::NEG_INFINITY);
        for p in v0.chunks_exact(3) {
            mn = mn.min(p[2]);
            mx = mx.max(p[2]);
        }
        let mut acc = 0f32;
        for i in 0..v0.len() {
            acc += (v1[i] - v0[i]).powi(2);
        }
        let rms = (acc / v0.len() as f32).sqrt();
        // Loop closure: RMS between first and last frame (a
        // cycle returns to its start pose).
        let mut acc2 = 0f32;
        for i in 0..v0.len() {
            acc2 += (v2[i] - v0[i]).powi(2);
        }
        let closure = (acc2 / v0.len() as f32).sqrt();
        println!(
            "{k:#010x}: {} frames, height {:.2}, poseRMS {:.3}, loopClosure {:.3} in {}",
            f + 1,
            mx - mn,
            rms,
            closure,
            sec.split("_wow_").next().unwrap_or(sec)
        );
    }
}
