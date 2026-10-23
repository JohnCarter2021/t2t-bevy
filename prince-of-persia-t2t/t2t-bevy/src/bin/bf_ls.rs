//! bf_ls — list (and optionally extract) a Jade BIG archive (.bf).
//!
//! Usage:
//!   bf_ls <archive.bf> [--limit N] [--extract <index> <outfile>]
//!   bf_ls <archive.bf> --stats        full M2 inventory: stream kinds,
//!                                     decompression totals, size.grs
//!                                     agreement, name-token buckets

use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::{classify, decompress_entry, parse_size_grs, EntryKind};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: bf_ls <archive.bf> [--limit N] [--extract <index> <outfile>]");
        return ExitCode::from(2);
    }
    let path = &args[1];
    let bf = match BigFile::open(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Some(p) = args.iter().position(|a| a == "--find-key") {
        let want = args
            .get(p + 1)
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0);
        let needle = want.to_le_bytes();
        for e in bf.active_files() {
            let raw = bf.read_data(e.index);
            let Ok(data) = t2t::jade_stream::decompress_entry(raw) else {
                continue;
            };
            let offs: Vec<usize> = data
                .windows(4)
                .enumerate()
                .filter(|(_, w)| *w == needle)
                .map(|(i, _)| i)
                .collect();
            if !offs.is_empty() {
                println!(
                    "key 0x{want:08x}: {} (entry {}) hits {} at {offs:?}",
                    e.name,
                    e.index,
                    offs.len()
                );
            }
        }
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "--stats") {
        return stats(&bf);
    }

    if let Some(p) = args.iter().position(|a| a == "--extract") {
        let idx: u32 = args.get(p + 1).and_then(|s| s.parse().ok()).unwrap_or(u32::MAX);
        let out = args.get(p + 2);
        match (bf.files.get(&idx), out) {
            (Some(e), Some(out)) => {
                let data = bf.read_data(e.index);
                if let Err(err) = std::fs::write(out, data) {
                    eprintln!("error writing {out}: {err}");
                    return ExitCode::FAILURE;
                }
                println!("extracted {} ({} bytes) -> {out}", e.name, data.len());
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("--extract needs a valid <index> and <outfile>");
                return ExitCode::from(2);
            }
        }
    }

    let limit: usize = args
        .iter()
        .position(|a| a == "--limit")
        .and_then(|p| args.get(p + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(25);

    println!(
        "BIG v{} | universe key 0x{:08x} | ext entry {} B | {} files ({} active) | {} dirs",
        bf.version,
        bf.universe_key,
        bf.ext_size,
        bf.files.len(),
        bf.active_files().len(),
        bf.active_dir_count()
    );
    println!("directories:");
    let mut dir_ids: Vec<u32> = bf.dirs.keys().copied().collect();
    dir_ids.sort();
    for id in &dir_ids {
        if let Some(d) = bf.dirs.get(id) {
            if !d.name.is_empty() {
                println!("  [{id}] {}", bf.dir_path(*id));
            }
        }
    }
    println!("first {limit} files:");
    for e in bf.active_files().iter().take(limit) {
        println!(
            "  [{:>5}] key 0x{:08x} {:>9} B  {}",
            e.index,
            e.key,
            e.length,
            bf.file_path(e)
        );
    }
    ExitCode::SUCCESS
}

fn mb(n: u64) -> String {
    format!("{:.1} MB", n as f64 / 1_048_576.0)
}

/// M2 inventory: classify every entry, fully decompress every LZO
/// stream, and cross-check terminators against size.grs.
fn stats(bf: &BigFile) -> ExitCode {
    use std::collections::BTreeMap;
    // size.grs lookup by archive key, if present.
    let grs = bf
        .active_files()
        .into_iter()
        .find(|e| e.name == "size.grs")
        .map(|e| parse_size_grs(bf.read_data(e.index)))
        .unwrap_or_default();

    let mut n_lzo = 0u64;
    let mut n_raw = 0u64;
    let mut n_decode_fail = 0u64;
    let mut stored_bytes = 0u64;
    let mut decompressed_bytes = 0u64;
    let mut grs_checked = 0u64;
    let mut grs_agree = 0u64;
    let mut buckets: BTreeMap<&'static str, (u64, u64)> = BTreeMap::new();
    let mut largest: Vec<(usize, String)> = Vec::new();

    for e in bf.active_files() {
        let data = bf.read_data(e.index);
        stored_bytes += data.len() as u64;
        let bucket = if e.name.contains("_wow_") {
            "world (wow)"
        } else if e.name.contains("_wol_") {
            "wol"
        } else if e.name.contains("GFX") {
            "graphics (GFX)"
        } else if e.name.contains("Sound") {
            "sound-named"
        } else {
            "other"
        };
        let b = buckets.entry(bucket).or_default();
        b.0 += 1;
        match classify(data, true) {
            EntryKind::Lzo {
                terminator,
                decompressed,
            } => {
                n_lzo += 1;
                b.1 += decompressed as u64;
                decompressed_bytes += decompressed as u64;
                if let Some(declared) = grs.get(&e.key) {
                    grs_checked += 1;
                    if *declared as usize == terminator {
                        grs_agree += 1;
                    }
                }
                match decompress_entry(data) {
                    Ok(out) if out.len() == decompressed => {
                        largest.push((out.len(), bf.file_path(e)));
                    }
                    _ => n_decode_fail += 1,
                }
            }
            EntryKind::Raw => {
                n_raw += 1;
                b.1 += data.len() as u64;
                decompressed_bytes += data.len() as u64;
                println!("  raw entry: [{}] {}", e.index, bf.file_path(e));
            }
        }
    }

    largest.sort_by(|a, b| b.0.cmp(&a.0));
    println!("=== M2 inventory ===");
    println!("entries:        {}", n_lzo + n_raw);
    println!("LZO streams:    {n_lzo}");
    println!("raw entries:    {n_raw}");
    println!("decode failures: {n_decode_fail}");
    println!("stored:         {}", mb(stored_bytes));
    println!("decompressed:   {}", mb(decompressed_bytes));
    println!(
        "size.grs:       {} keys, {} entries checked, {} terminator agreements",
        grs.len(),
        grs_checked,
        grs_agree
    );
    println!("buckets (name-token -> count, decompressed):");
    for (name, (count, bytes)) in &buckets {
        println!("  {name:<14} {count:>5}  {}", mb(*bytes));
    }
    println!("largest decompressed entries:");
    for (len, path) in largest.iter().take(10) {
        println!("  {:>10} B  {}", len, path);
    }
    ExitCode::SUCCESS
}
