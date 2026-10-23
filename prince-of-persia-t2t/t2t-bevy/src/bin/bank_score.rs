//! The Prince's animation bank (AMIR_wow record
//! 0x2d000acc) lists every descriptor in order. This
//! dumps the table (slot, descriptor, clip, duration)
//! and scores every unique clip on the Prince rig:
//! frames + loop-closure RMS (pose at frame 0 vs the
//! last frame) + stride (pose distance frame 0 vs the
//! middle frame — a real cycle differs mid-way).
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn lef32(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn sector(bf: &BigFile, substr: &str) -> Vec<u8> {
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(substr))
        .unwrap_or_else(|| panic!("sector {substr}"));
    decompress_entry(bf.read_data(e.index)).expect("decompress")
}

fn pose_rms(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len()) / 3;
    if n == 0 {
        return f32::INFINITY;
    }
    let mut s = 0.0f32;
    for i in 0..n * 3 {
        let d = a[i] - b[i];
        s += d * d;
    }
    (s / n as f32).sqrt()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let amir = sector(&bf, "AMIR_wow");
    let recs = walk_wow(&amir);
    let by_key: std::collections::HashMap<u32, usize> =
        recs.iter().enumerate().map(|(i, r)| (r.key, i)).collect();
    let bank = &recs[by_key[&0x2d000acc]];
    let end = (bank.body_off + bank.size as usize + 12).min(amir.len());
    // Table entries: descriptor keys in offset order.
    let mut table: Vec<(u32, u32, f32)> = Vec::new(); // desc, clip, duration
    let mut off = bank.body_off;
    while off + 4 <= end {
        let v = le32(&amir, off);
        if let Some(di) = by_key.get(&v) {
            let d = &recs[*di];
            let ty = le32(&amir, d.body_off);
            if d.data_range().len() >= 40 && ty & 0xffff == 0xff01 {
                let clip = (le32(&amir, d.data_range().start) << 16) | (ty >> 16);
                let dur = lef32(&amir, d.data_range().start + 28);
                table.push((v, clip, dur));
            }
        }
        off += 4;
    }
    println!("bank slots: {}", table.len());
    for (i, (dk, ck, dur)) in table.iter().enumerate() {
        println!("slot {i:3}: desc {dk:#010x} clip {ck:#010x} dur {dur:.2}");
    }
    // Prince rig.
    let rig_data = sector(&bf, "PrinceFinal_Shape");
    let ch = Character::load(&rig_data, 0x510022d3, "B_Pr").expect("prince rig");
    // Score unique clips.
    let mut seen: Vec<u32> = Vec::new();
    for (_, ck, _) in &table {
        if !seen.contains(ck) {
            seen.push(*ck);
        }
    }
    let mut scored: Vec<(f32, u32, u32, f32)> = Vec::new(); // loopRMS, clip, frames, midRMS
    for ck in &seen {
        if let Some(ci) = by_key.get(ck) {
            let cr = &recs[*ci];
            let clip = Clip::load(&amir[cr.data_range()]);
            let f0 = t2t::skin::posed_verts(&ch, &clip, &ChannelMap::Identity, 0);
            let fl = t2t::skin::posed_verts(&ch, &clip, &ChannelMap::Identity, clip.max_frame);
            let fm = t2t::skin::posed_verts(&ch, &clip, &ChannelMap::Identity, clip.max_frame / 2);
            scored.push((pose_rms(&f0, &fl), *ck, clip.max_frame, pose_rms(&f0, &fm)));
        }
    }
    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    println!("\nbest loop closures (rms, clip, frames, mid-dist):");
    for (rms, ck, fr, mid) in scored.iter().take(30) {
        println!("  rms {rms:7.3} clip {ck:#010x} frames {fr:3} mid {mid:7.3}");
    }
    // Root-translation fingerprint: locomotion cycles
    // carry steady horizontal root travel in channel 0.
    println!("\nroot-translation travel (clip, frames, net dx dy dz, path):");
    for ck in &seen {
        if let Some(ci) = by_key.get(ck) {
            let cr = &recs[*ci];
            let clip = Clip::load(&amir[cr.data_range()]);
            if let Some(keys) = clip.trans.get(&0) {
                if keys.len() >= 2 {
                    let a = keys.first().unwrap().value;
                    let b = keys.last().unwrap().value;
                    let net = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let mut path = 0.0f32;
                    for w in keys.windows(2) {
                        let d = [
                            w[1].value[0] - w[0].value[0],
                            w[1].value[1] - w[0].value[1],
                            w[1].value[2] - w[0].value[2],
                        ];
                        path += (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    }
                    let horiz = (net[0] * net[0] + net[1] * net[1]).sqrt();
                    if path > 30.0 {
                        println!(
                            "  clip {ck:#010x} frames {:3} net [{:7.1} {:7.1} {:7.1}] horiz {horiz:6.1} path {path:6.1}",
                            clip.max_frame, net[0], net[1], net[2]
                        );
                    }
                }
            }
        }
    }
    let mut movers: Vec<&(f32, u32, u32, f32)> = scored
        .iter()
        .filter(|s| s.2 >= 12 && s.0 < 1.5)
        .collect();
    movers.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());
    println!("\nbiggest mid-motion among plausible loops:");
    for (rms, ck, fr, mid) in movers.iter().take(25) {
        println!("  rms {rms:7.3} clip {ck:#010x} frames {fr:3} mid {mid:7.3}");
    }
    ExitCode::SUCCESS
}
