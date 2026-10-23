//! Playback parity: the library skinning/clip pipeline must
//! reproduce pose_test's verified renders (same AABBs).

use std::path::PathBuf;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn quai() -> Option<Vec<u8>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let bf_path = std::env::var("T2T_POP3_BF").map(PathBuf::from).unwrap_or_else(|_| {
        root.join("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf")
    });
    let bf = BigFile::open(bf_path.to_str()?).ok()?;
    let e = bf.active_files().into_iter().find(|e| e.name.contains("100_Quai"))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

fn aabb(v: &[f32]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for p in v.chunks_exact(3) {
        for i in 0..3 {
            mn[i] = mn[i].min(p[i]);
            mx[i] = mx[i].max(p[i]);
        }
    }
    (mn, mx)
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 0.02)
}

#[test]
fn kaileena_clip_a0_frame200_parity() {
    let Some(data) = quai() else {
        eprintln!("pop3.bf not present; skipping");
        return;
    };
    let ch = Character::load(&data, 0x420035b0, "B_Kaileena").expect("character");
    let recs = walk_wow(&data);
    let clip_rec = recs.iter().find(|r| r.key == 0x420035a0).expect("clip");
    let clip = Clip::load(&data[clip_rec.data_range()]);
    let hdr = recs.iter().find(|r| r.key == 0x42000419).expect("header");
    let map = ChannelMap::from_header(&data[hdr.data_range()]);
    let verts = posed_verts(&ch, &clip, &map, 200);
    let (mn, mx) = aabb(&verts);
    // pose_test --render anim200 (verified render anim200_rot.png)
    assert!(close(mn, [-0.33, -1.55, -0.01]), "min {mn:?}");
    assert!(close(mx, [0.45, 0.56, 1.81]), "max {mx:?}");
}

#[test]
fn soldier_clip_6d46_identity_parity() {
    let Some(data) = quai() else {
        eprintln!("pop3.bf not present; skipping");
        return;
    };
    let ch = Character::load(&data, 0x42003196, "B_BabylonSoldier").expect("character");
    let recs = walk_wow(&data);
    let clip_rec = recs.iter().find(|r| r.key == 0x42006d46).expect("clip");
    let clip = Clip::load(&data[clip_rec.data_range()]);
    let verts = posed_verts(&ch, &clip, &ChannelMap::Identity, 10);
    let (mn, mx) = aabb(&verts);
    // pose_test soldier_id_f10.png
    assert!(close(mn, [-0.71, -0.64, 0.78]), "min {mn:?}");
    assert!(close(mx, [0.94, 0.28, 1.83]), "max {mx:?}");
}
