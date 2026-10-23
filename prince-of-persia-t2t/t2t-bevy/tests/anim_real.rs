//! Animation hunt verification against the real 100_Quai world:
//! the 72-byte-unit pose records (gro 0x20006 / 0x20036 families).
//! Numbers measured 2026-10-22 — see docs/FORMATS.md hunt log.

use t2t::anim::{is_pose_record, parse_pose_record};
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

#[test]
fn quai_pose_records_decode() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf");
    if !p.exists() {
        eprintln!("pop3.bf not present — skipping");
        return;
    }
    let bf = BigFile::open(&p).expect("archive");
    let entry = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("100_Quai"))
        .expect("100_Quai entry");
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    let records = walk_wow(&data);

    // Record key 0x4200359f (Kaileena family, gro 0x20036): a
    // 35-unit pose record.
    let rec = records
        .iter()
        .find(|r| r.key == 0x4200_359f)
        .expect("pose record");
    let sub = &data[rec.data_range()];
    assert!(is_pose_record(sub));
    let units = parse_pose_record(sub).expect("units");
    assert_eq!(units.len(), 35);

    // First unit is the root/header unit: index 0xFFFF, zero
    // translation. The rest index bones 0..=33 sequentially.
    assert_eq!(units[0].index, 0xFFFF);
    assert_eq!(units[0].translation, [0.0, 0.0, 0.0]);
    for (i, u) in units.iter().enumerate().skip(1) {
        assert_eq!(u.index as usize, i - 1);
    }

    // A known decoded unit (measured during the hunt): bone 1
    // carries translation ~(0.1021, -0.0406, 0.0102).
    let b1 = &units[2];
    assert!((b1.translation[0] - 0.1021).abs() < 0.001);
    assert!((b1.translation[1] + 0.0406).abs() < 0.001);
    assert!((b1.translation[2] - 0.0102).abs() < 0.001);

    // Every translation is finite and humanoid-scale (< 5 m).
    for u in &units {
        for v in u.translation {
            assert!(v.is_finite() && v.abs() < 5.0);
        }
    }

    // Rotations decode as unit quaternions whose reconstructed
    // largest component really is the largest (smallest-three
    // invariant), and the root unit's rotation is the identity —
    // its raw value 0xe0080200 is the canonical identity encoding.
    for u in &units {
        let q = u.rotation;
        let n = q.iter().map(|c| c * c).sum::<f32>();
        assert!((n - 1.0).abs() < 0.02, "unit quat norm {n}");
        let idx = (u.rotation_raw >> 30) as usize;
        for (i, c) in q.iter().enumerate() {
            if i != idx {
                assert!(q[idx] + 0.02 >= c.abs(), "smallest-three invariant");
            }
        }
    }
    assert_eq!(units[0].rotation_raw, 0xe008_0200);
    assert!(units[0].rotation[3] > 0.999);

    // The small Kaileena record 0x420035a5 (gro 0x20006) is a
    // 3-unit pose record of the same family.
    let rec3 = records
        .iter()
        .find(|r| r.key == 0x4200_35a5)
        .expect("small pose record");
    let units3 = parse_pose_record(&data[rec3.data_range()]).expect("units3");
    assert_eq!(units3.len(), 3);
}
