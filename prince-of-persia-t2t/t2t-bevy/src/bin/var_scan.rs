//! Census of Jade var refs (type 0x0200) across AMIR
//! behavior records: which var ids each record mentions,
//! and — with `common` — the var ids shared by a set of
//! records, with their occurrence cells.
//! usage: var_scan <bf> [common <key-hex>...]
//! With no `common` args, prints per-record var counts
//! for the three movement-related records only.
use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let focus: Vec<u32> = if args.get(2).map(|s| s.as_str()) == Some("common") {
        args[3..]
            .iter()
            .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("key hex"))
            .collect()
    } else {
        vec![0x66038dd9, 0xb40001a2, 0x66038de7]
    };
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("AMIR_wow"))
        .expect("amir entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    // record key -> var id -> cells
    let mut per_rec: BTreeMap<u32, BTreeMap<u16, Vec<usize>>> = BTreeMap::new();
    for r in walk_wow(&data) {
        if !focus.contains(&r.key) {
            continue;
        }
        if r.size < 20 {
            continue;
        }
        let body = r.body_off;
        let rec_end = (body + r.size as usize).min(data.len());
        if body + 16 >= rec_end {
            continue;
        }
        let pool = body + le32(&data, body + 4) as usize;
        let end = pool.min(rec_end);
        if end <= body + 16 {
            continue;
        }
        let ncells = (end - (body + 16)) / 4;
        let mut vars: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
        for i in 0..ncells {
            let w = le32(&data, body + 16 + i * 4);
            let (id, ty) = ((w & 0xffff) as u16, (w >> 16) as u16);
            if ty == 0x0200 {
                vars.entry(id).or_default().push(i);
            }
        }
        per_rec.insert(r.key, vars);
    }
    if args.get(2).map(|s| s.as_str()) == Some("common") {
        // Intersection of var ids across all focus records.
        let mut common: Option<BTreeSet<u16>> = None;
        for vars in per_rec.values() {
            let ids: BTreeSet<u16> = vars.keys().copied().collect();
            common = Some(match common {
                None => ids,
                Some(c) => c.intersection(&ids).copied().collect(),
            });
        }
        let common = common.unwrap_or_default();
        println!("shared var ids ({}):", common.len());
        for id in &common {
            let counts: Vec<String> = per_rec
                .iter()
                .map(|(k, v)| format!("{k:#x}:{}", v.get(id).map(|c| c.len()).unwrap_or(0)))
                .collect();
            println!("  var:{id:x}  {}", counts.join(" "));
        }
        return ExitCode::SUCCESS;
    }
    for (k, vars) in &per_rec {
        let mut line = format!("rec {k:#010x}: {} distinct vars:", vars.len());
        for (id, cells) in vars {
            line.push_str(&format!(" {id:x}({})", cells.len()));
        }
        println!("{line}");
    }
    ExitCode::SUCCESS
}
