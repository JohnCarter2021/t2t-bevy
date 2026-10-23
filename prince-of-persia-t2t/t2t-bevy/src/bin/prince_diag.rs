//! prince_diag — for each GEO skin bone i, find which
//! collected GAO j makes geo_global_j * bind_i closest to
//! identity. Reveals order permutations vs hierarchy breaks.
//!
//! Usage: prince_diag <pop3.bf> <sector-substr> <geo-key-hex> <prefix>

use t2t::bigfile::BigFile;
use t2t::gao::mat_mul;
use t2t::jade_stream::decompress_entry;
use t2t::skin::Character;

fn dev(a: &[f32; 16]) -> f32 {
    // Max deviation from identity over the 3x4 part (+ m[15]~0 quirk ignored).
    let mut d = 0f32;
    for r in 0..3 {
        for c in 0..4 {
            let want = if r == c { 1.0 } else { 0.0 };
            d = d.max((a[r * 4 + c] - want).abs());
        }
    }
    d
}

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage");
    let substr = std::env::args().nth(2).expect("usage");
    let geo_key = u32::from_str_radix(std::env::args().nth(3).expect("usage").trim_start_matches("0x"), 16).expect("hex");
    let prefix = std::env::args().nth(4).expect("usage");
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf.active_files().into_iter().find(|e| e.name.contains(&substr)).expect("sector");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let ch = Character::load(&data, geo_key, &prefix).expect("character");
    println!("collected {} GAO bones; skin has {} bones", ch.bones.len(), ch.skin.len());
    let locals = ch.bind_locals();
    let globals = ch.compose_globals(&locals);
    let geo_globals: Vec<[f32; 16]> = globals
        .iter()
        .map(|g| mat_mul(&ch.root_global, g))
        .collect();
    for (i, (bidx, bind)) in ch.skin.iter().enumerate() {
        let mut best = (usize::MAX, f32::INFINITY);
        for (j, gg) in geo_globals.iter().enumerate() {
            let s = mat_mul(gg, bind);
            let d = dev(&s);
            if d < best.1 {
                best = (j, d);
            }
        }
        let self_dev = if (*bidx as usize) < geo_globals.len() {
            dev(&mat_mul(&geo_globals[*bidx as usize], bind))
        } else {
            f32::INFINITY
        };
        let name = |j: usize| {
            ch.bones.get(j).map(|b| b.1.as_str()).unwrap_or("?")
        };
        println!(
            "skin {i:2} (bone_idx {bidx:2} {:28}) self-dev {:.4} | best j={} ({}) dev {:.4}",
            name(*bidx as usize),
            self_dev,
            best.0,
            name(best.0),
            best.1
        );
    }
}
