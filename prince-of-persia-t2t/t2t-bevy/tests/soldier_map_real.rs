//! The shared enemy pool on the Babylonian Soldier via
//! the name-based channel map: 34 of the Sand Garde's 46
//! channels pair up by Bip bone name, and the mapped pose
//! of the lunging cycle is a coherent humanoid (bounded,
//! finite, and DIFFERENT from the folded identity pose).
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::wow::walk_wow;

fn load(bf: &BigFile, substr: &str) -> Vec<u8> {
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(substr))
        .unwrap();
    decompress_entry(bf.read_data(e.index)).unwrap()
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

#[test]
fn pool_plays_on_soldier_via_name_map() {
    let bf = BigFile::open("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf")
        .expect("archive");
    let sg_data = load(&bf, "ACT_Enemy_SandGarde");
    let sg = Character::load(&sg_data, 0x51001f50, "B_SG").unwrap();
    let quai = load(&bf, "100_Quai");
    let so = Character::load(&quai, 0x42003196, "B_BabylonSoldier").unwrap();

    let map = ChannelMap::from_bone_names(&sg, "B_SG", &so, "B_BabylonSoldier");
    let pairs = map.pairs(so.bones.len());
    assert_eq!(pairs.len(), 34, "34 Bip bones pair by name");

    let recs = walk_wow(&sg_data);
    let r = recs.iter().find(|r| r.key == 0x6c0019e5).unwrap();
    let clip = Clip::load(&sg_data[r.data_range()]);
    let mapped = posed_verts(&so, &clip, &map, 7);
    assert!(mapped.iter().all(|v| v.is_finite()));
    let (mn, mx) = aabb(&mapped);
    let height = mx[2] - mn[2];
    let width = ((mx[0] - mn[0]).powi(2) + (mx[1] - mn[1]).powi(2)).sqrt();
    // A lunging humanoid: bounded in both axes (the
    // identity pose folds him into a compact knot with a
    // distinctly smaller footprint).
    assert!(height < 2.2 && width > 0.9, "mapped pose extent h={height:.2} w={width:.2}");
    let ident = posed_verts(&so, &clip, &ChannelMap::Identity, 7);
    let differ = mapped
        .iter()
        .zip(ident.iter())
        .filter(|(a, b)| (*a - *b).abs() > 0.05)
        .count();
    assert!(differ > 200, "the map must actually move bones ({differ} verts differ)");
}
