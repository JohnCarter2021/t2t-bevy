//! The resolve_banks census, locked: every rostered
//! enemy beyond the original trio pairs the Sand Garde's
//! channels onto its own rig by Bip bone name with the
//! censused count, and the shared pool's lunging cycle
//! poses it as a bounded, finite humanoid (the strips in
//! out/resolve/ carry the visual verdicts).
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

/// (bank, geo, prefix, expected name-map pairs of 46)
const ROSTER: &[(&str, u32, &str, usize)] = &[
    ("ACT_Enemy_SandGateGarde", 0x51001f9d, "B_SGG", 42),
    ("ACT_Enemy_Thrall", 0x51003f51, "B_Thrall", 39),
    ("ACT_Enemy_DarkWarrior", 0x5100417f, "B_DW", 36),
    ("ACT_Enemy_SandArcher", 0x51002023, "B_SA", 35),
    ("ACT_Enemy_HumanArcher", 0x51001bdc, "B_HA", 35),
    ("ACT_Enemy_Twins", 0x42002e1c, "B_Twins", 34),
    ("ACT_Enemy_Twins", 0x42002e93, "B_Twins", 34),
    ("ACT_Enemy_Enchantress", 0x42002ac9, "B_Enchantress", 31),
];

/// Mahasti is the roster's identity exception: her own
/// bank's 0x6603xxxx moveset plays on B_KA unmapped (the
/// shared pool slabs her loincloth instead). All four
/// combat clips must pose finite and humanoid-bounded.
#[test]
fn mahasti_own_clips_play_by_identity() {
    let bf = BigFile::open("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf")
        .expect("archive");
    let data = load(&bf, "ACT_Enemy_Mahasti");
    let ch = Character::load(&data, 0x42002c7d, "B_KA").unwrap();
    let recs = walk_wow(&data);
    for key in [0x6603a66au32, 0x6603a66d, 0x6603a649, 0x6603a3f6] {
        let r = recs.iter().find(|r| r.key == key).unwrap();
        let clip = Clip::load(&data[r.data_range()]);
        let v = posed_verts(&ch, &clip, &ChannelMap::Identity, clip.max_frame);
        assert!(v.iter().all(|x| x.is_finite()), "mahasti {key:#x} pose is finite");
        let (mn, mx) = aabb(&v);
        let h = mx[2] - mn[2];
        let w = ((mx[0] - mn[0]).powi(2) + (mx[1] - mn[1]).powi(2)).sqrt();
        assert!(
            (0.3..=2.6).contains(&h) && w <= 3.6,
            "mahasti {key:#x} end pose is humanoid-bounded (h {h:.2}, w {w:.2})"
        );
    }
}

#[test]
fn roster_rigs_pair_and_pose_via_name_map() {
    let bf = BigFile::open("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf")
        .expect("archive");
    let sg_data = load(&bf, "ACT_Enemy_SandGarde");
    let sg = Character::load(&sg_data, 0x51001f50, "B_SG").unwrap();
    let recs = walk_wow(&sg_data);
    let cyc = recs.iter().find(|r| r.key == 0x6c0019e5).unwrap();
    let cycle = Clip::load(&sg_data[cyc.data_range()]);
    for &(bank, geo, prefix, want_pairs) in ROSTER {
        let data = load(&bf, bank);
        let ch = Character::load(&data, geo, prefix)
            .unwrap_or_else(|| panic!("{bank} rig {geo:#x} loads"));
        let map = ChannelMap::from_bone_names(&sg, "B_SG", &ch, prefix);
        let pairs = map.pairs(ch.bones.len()).len();
        assert_eq!(pairs, want_pairs, "{bank} name-map pair count");
        let v = posed_verts(&ch, &cycle, &map, 7);
        assert!(v.iter().all(|x| x.is_finite()), "{bank} pose is finite");
        let (mn, mx) = aabb(&v);
        let h = mx[2] - mn[2];
        let w = ((mx[0] - mn[0]).powi(2) + (mx[1] - mn[1]).powi(2)).sqrt();
        assert!(
            (0.6..=1.6).contains(&h) && w < 3.4,
            "{bank} posed lunge is humanoid-bounded (h {h:.2}, w {w:.2})"
        );
    }
}
