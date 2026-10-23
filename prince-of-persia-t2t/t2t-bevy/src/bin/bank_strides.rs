//! Stride census over the Prince's animation bank:
//! for every unique bank clip, measure the posed AABB
//! height (upright?) at frame 0 / mid / last, and the
//! vertical travel of both Foot bones across the clip
//! (alternating strides move them). Prints clips that
//! look like upright gait cycles.
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, pose_locals, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn sector(bf: &BigFile, substr: &str) -> Vec<u8> {
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(substr))
        .unwrap_or_else(|| panic!("sector {substr}"));
    decompress_entry(bf.read_data(e.index)).expect("decompress")
}

fn aabb_h(v: &[f32]) -> (f32, f32) {
    let mut mn = f32::INFINITY;
    let mut mx = f32::NEG_INFINITY;
    for p in v.chunks_exact(3) {
        mn = mn.min(p[2]);
        mx = mx.max(p[2]);
    }
    (mx - mn, mn)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let amir = sector(&bf, "AMIR_wow");
    let recs = walk_wow(&amir);
    let by_key: std::collections::HashMap<u32, usize> =
        recs.iter().enumerate().map(|(i, r)| (r.key, i)).collect();
    // Bank clips in slot order.
    let bank = &recs[by_key[&0x2d000acc]];
    let end = (bank.body_off + bank.size as usize + 12).min(amir.len());
    let mut clips: Vec<u32> = Vec::new();
    let mut off = bank.body_off;
    while off + 4 <= end {
        let v = le32(&amir, off);
        if let Some(di) = by_key.get(&v) {
            let d = &recs[*di];
            let ty = le32(&amir, d.body_off);
            if d.data_range().len() >= 40 && ty & 0xffff == 0xff01 {
                let ck = (le32(&amir, d.data_range().start) << 16) | (ty >> 16);
                if !clips.contains(&ck) {
                    clips.push(ck);
                }
            }
        }
        off += 4;
    }
    // Targeted mode: STRIDE_KEYS=<file> scores exactly the
    // hex keys in the file (records resolved inside AMIR).
    if let Ok(kf) = std::env::var("STRIDE_KEYS") {
        let txt = std::fs::read_to_string(&kf).expect("keys file");
        clips = txt
            .lines()
            .filter_map(|l| u32::from_str_radix(l.trim(), 16).ok())
            .collect();
        println!("targeted: {} keys from {kf}", clips.len());
    }
    // Archive-wide mode: STRIDE_ALL=1 scores every clip
    // record in the archive, not just bank members.
    let all = std::env::var("STRIDE_ALL").is_ok();
    let mut clip_data: std::collections::HashMap<u32, Vec<u8>> = std::collections::HashMap::new();
    if all {
        clips.clear();
        for e in bf.active_files() {
            if let Ok(d) = decompress_entry(bf.read_data(e.index)) {
                for r in walk_wow(&d) {
                    if matches!(r.gro_type, Some(g) if (0x20020..0x20050).contains(&g))
                        && !clip_data.contains_key(&r.key)
                    {
                        clip_data.insert(r.key, d[r.data_range()].to_vec());
                        clips.push(r.key);
                    }
                }
            }
        }
        println!("archive-wide: {} unique clips", clips.len());
    }
    let rdata = sector(&bf, "PrinceFinal_Shape");
    let ch = Character::load(&rdata, 0x510022d3, "B_Pr").expect("prince rig");
    let feet: Vec<usize> = ch
        .bones
        .iter()
        .enumerate()
        .filter(|(_, b)| b.1.contains("Foot"))
        .map(|(i, _)| i)
        .collect();
    println!("foot bones: {:?}", feet.iter().map(|&i| ch.bones[i].1.clone()).collect::<Vec<_>>());
    println!("clip        frames  h0    hmid  footL-range footR-range  minz");
    for ck in &clips {
        let clip = if all {
            Clip::load(&clip_data[ck])
        } else {
            let Some(ci) = by_key.get(ck) else { continue };
            Clip::load(&amir[recs[*ci].data_range()])
        };
        if clip.max_frame < 8 {
            continue;
        }
        let v0 = posed_verts(&ch, &clip, &ChannelMap::Identity, 0);
        let vm = posed_verts(&ch, &clip, &ChannelMap::Identity, clip.max_frame / 2);
        let (h0, _) = aabb_h(&v0);
        let (hm, mnz) = aabb_h(&vm);
        // Foot bone z travel over the clip (8 samples).
        let mut ranges = [0.0f32; 2];
        for (fi, &fb) in feet.iter().take(2).enumerate() {
            let mut zs: Vec<f32> = Vec::new();
            for s in 0..8 {
                let fr = clip.max_frame * s / 7;
                let locals = pose_locals(&ch, &clip, &ChannelMap::Identity, fr);
                let globals = ch.compose_globals(&locals);
                zs.push(globals[fb][14]);
            }
            ranges[fi] = zs.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - zs.iter().copied().fold(f32::INFINITY, f32::min);
        }
        let stride = ranges[0] + ranges[1];
        if h0 > 1.45 && hm > 1.45 && stride > 0.18 {
            println!(
                "{ck:#010x}  {:4}   {h0:.2}  {hm:.2}   {:.2}        {:.2}        {mnz:.2}",
                clip.max_frame, ranges[0], ranges[1]
            );
        }
    }
    ExitCode::SUCCESS
}
