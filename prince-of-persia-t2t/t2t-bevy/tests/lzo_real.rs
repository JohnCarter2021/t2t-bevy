//! M2 verification against the real pop3.bf: LZO streams decode to the
//! sizes their block headers promise, terminator offsets agree with
//! size.grs, and known raw entries are classified Raw.

use t2t::bigfile::BigFile;
use t2t::jade_stream::{classify, decompress_entry, parse_size_grs, EntryKind};

fn bf() -> Option<BigFile> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf");
    if p.exists() {
        BigFile::open(&p).ok()
    } else {
        None
    }
}

#[test]
fn lzo_streams_decode_and_agree_with_size_grs() {
    let Some(bf) = bf() else {
        eprintln!("pop3.bf not present — skipping");
        return;
    };
    // Entry 8: 620_ExitTo630_GFX_wow — one LZO block, dec 30,686.
    let d8 = bf.read_data(8);
    let out = decompress_entry(d8).expect("entry 8 must decompress");
    assert_eq!(out.len(), 30686);

    // size.grs (entry 6647) — declared length == terminator offset.
    let grs_raw = bf.read_data(6647);
    let grs = parse_size_grs(grs_raw);
    assert_eq!(grs.len(), 6642);
    match classify(d8, true) {
        EntryKind::Lzo { terminator, .. } => {
            assert_eq!(terminator as u32, grs[&0xff03_e214]);
        }
        EntryKind::Raw => panic!("entry 8 must classify as LZO"),
    }

    // Known raw entries classify Raw: ps2.ico (1), versiontag.bin (2).
    assert_eq!(classify(bf.read_data(1), true), EntryKind::Raw);
    assert_eq!(classify(bf.read_data(2), true), EntryKind::Raw);
}
