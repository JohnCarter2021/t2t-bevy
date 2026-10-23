//! Integration test against the user's real pop3.bf, when present.
//! Set T2T_POP3_BF to override the default location; skips cleanly otherwise.

use t2t::bigfile::BigFile;

fn pop3_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("T2T_POP3_BF") {
        return Some(p.into());
    }
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

#[test]
fn real_pop3_bf_parses() {
    let Some(path) = pop3_path() else {
        eprintln!("pop3.bf not present — skipping real-archive test");
        return;
    };
    let bf = BigFile::open(&path).expect("pop3.bf should parse");
    // Verified byte-by-byte 2026-10-22 against the GOG build.
    assert_eq!(bf.version, 38);
    assert_eq!(bf.universe_key, 0xcc00_008e);
    assert_eq!(bf.ext_size, 84);
    assert_eq!(bf.active_files().len(), 6648);
    assert_eq!(bf.active_dir_count(), 5);
    // Root dir must be named ROOT; first file is a known entry.
    let root = bf.dirs.get(&0).expect("root dir");
    assert_eq!(root.name, "ROOT");
    let f0 = bf.files.get(&0).expect("file 0");
    assert_eq!(f0.name, "ps2old.ico");
    assert_eq!(f0.length, 59388);
    // Data round-trip: entry bytes must be readable and non-empty.
    assert_eq!(bf.read_data(0).len(), 59388);
}
