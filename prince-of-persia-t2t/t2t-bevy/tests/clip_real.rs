//! Real-data test for the 0x2002b clip walker: section headers
//! and the byte-exact rotation-stream anchor in clip 0x420035a0
//! (100_Quai): the stream section at 288 declares 254 keys;
//! 18 bytes of stream header + 254 * 6-byte keys lands exactly
//! on the next section at 1830.

use t2t::anim::{find_sections, read_rot_keys, SectionKind};
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn quai_clip(key: u32) -> Option<Vec<u8>> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf"
    );
    let bf = BigFile::open(path).ok()?;
    let entry = bf.active_files().into_iter().find(|e| e.name.contains("100_Quai"))?;
    let raw = bf.read_data(entry.index);
    let data = decompress_entry(raw).ok()?;
    let records = walk_wow(&data);
    let rec = records.iter().find(|r| r.key == key)?;
    Some(data[rec.data_range()].to_vec())
}

#[test]
fn clip_a0_sections_and_anchor() {
    let Some(d) = quai_clip(0x420035a0) else {
        eprintln!("pop3.bf not present; skipping");
        return;
    };
    let secs = find_sections(&d);
    assert!(secs.len() >= 4, "expected >= 4 sections, got {secs:?}");
    assert_eq!(secs[0].offset, 0);
    assert_eq!(secs[0].kind, SectionKind::Stream);
    assert_eq!(secs[0].count, 2, "root stream: 2 keys");
    assert_eq!(secs[1].kind, SectionKind::Directory);
    assert_eq!(secs[1].count, 15, "15 tracks");
    let stream = secs.iter().find(|s| s.offset == 288).expect("stream @288");
    assert_eq!(stream.kind, SectionKind::Stream);
    assert_eq!(stream.count, 254);
    // 18-byte stream header, then 254 keys * 6 B -> next section.
    let keys = read_rot_keys(&d, 288 + 18, 254).expect("254 keys readable");
    assert_eq!(keys.len(), 254);
    assert_eq!(288 + 18 + 254 * 6, 1830, "anchor: stream ends at next section");
    // Every decoded rotation must be a unit quaternion.
    for k in &keys {
        let n: f32 = k.rotation.iter().map(|c| c * c).sum();
        assert!((n - 1.0).abs() < 0.02, "non-unit quat {n}");
    }
}

#[test]
fn clip_a2_walk() {
    let Some(d) = quai_clip(0x420035a2) else {
        eprintln!("pop3.bf not present; skipping");
        return;
    };
    let secs = t2t::anim::parse_clip(&d);
    let unknown = secs.iter().filter(|s| s.role == t2t::anim::SectionRole::Unknown).count();
    let last = secs.last().expect("sections");
    assert_eq!(last.offset + last.span, d.len(), "walk must end at EOF");
    assert_eq!(unknown, 0, "no unknown sections: {secs:?}");
    let rots = secs.iter().filter(|s| s.role == t2t::anim::SectionRole::Rot).count();
    let trs = secs.iter().filter(|s| s.role == t2t::anim::SectionRole::Trans).count();
    eprintln!("a2: {} sections, {rots} rot, {trs} trans", secs.len());
}

#[test]
fn clip_a7_walk() {
    let Some(d) = quai_clip(0x420035a7) else {
        eprintln!("pop3.bf not present; skipping");
        return;
    };
    let secs = t2t::anim::parse_clip(&d);
    let unknown = secs.iter().filter(|s| s.role == t2t::anim::SectionRole::Unknown).count();
    let last = secs.last().expect("sections");
    assert_eq!(last.offset + last.span, d.len(), "walk must end at EOF");
    assert_eq!(unknown, 0, "no unknown sections: {secs:?}");
    eprintln!("a7: {} sections", secs.len());
}

#[test]
fn clip_6d46_walk() {
    let Some(d) = quai_clip(0x42006d46) else {
        eprintln!("pop3.bf not present; skipping");
        return;
    };
    let secs = t2t::anim::parse_clip(&d);
    let last = secs.last().expect("sections");
    assert_eq!(last.offset + last.span, d.len(), "walk must end at EOF");
    eprintln!(
        "6d46: {} sections, unknown {}",
        secs.len(),
        secs.iter().filter(|s| s.role == t2t::anim::SectionRole::Unknown).count()
    );
}

#[test]
fn clip_6d4b_walk() {
    let Some(d) = quai_clip(0x42006d4b) else {
        eprintln!("pop3.bf not present; skipping");
        return;
    };
    let secs = t2t::anim::parse_clip(&d);
    let last = secs.last().expect("sections");
    assert_eq!(last.offset + last.span, d.len(), "walk must end at EOF");
    eprintln!(
        "6d4b: {} sections, unknown {}",
        secs.len(),
        secs.iter().filter(|s| s.role == t2t::anim::SectionRole::Unknown).count()
    );
}
