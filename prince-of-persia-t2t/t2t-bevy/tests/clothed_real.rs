//! Clothed Prince (GEO 0x6d002617, PrinceLoading_Shape).
//! The loading sector stores its B_Pr rig posed — the
//! mesh is a collapsed loading-screen statue — but its
//! bind matrices exactly encode that posed skeleton:
//! reconstruct_bind() recovers it with zero error.
//! Canonical-clip playback on this body is NOT locked
//! here: the loading export's per-bone bind geometry
//! differs from the canonical rig under every rigid
//! frame transform tried (translations x z-rotations x
//! mirror; best tracklist AABB mismatch 3.2), so the
//! body ships as a recovered statue, not a playable
//! variant. See docs/FORMATS.md (2026-10-23).
use std::path::PathBuf;
use t2t::bigfile::BigFile;
use t2t::gao::mat_inv;
use t2t::jade_stream::decompress_entry;
use t2t::skin::Character;

fn bf_path() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.join("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf")
}

fn sector(bf: &BigFile, substr: &str) -> Vec<u8> {
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(substr))
        .unwrap_or_else(|| panic!("sector {substr}"));
    decompress_entry(bf.read_data(e.index)).expect("decompress")
}

#[test]
fn clothed_prince_bind_reconstructs_exactly() {
    let path = bf_path();
    if !path.exists() {
        eprintln!("pop3.bf not present; skipping");
        return;
    }
    let bf = BigFile::open(path.to_str().unwrap()).expect("archive");
    let loading = sector(&bf, "PrinceLoading_Shape");
    let mut ch = Character::load(&loading, 0x6d002617, "B_Pr").expect("clothed load");
    assert_eq!(ch.skin.len(), 39);
    // The skin list's bone_idx values address the
    // B_Pr-filtered collection: every bind origin sits
    // among the verts that entry weights.
    for (pos, (bidx, bind)) in ch.skin.iter().enumerate() {
        let inv = mat_inv(bind).expect("bind invertible");
        let origin = [inv[12], inv[13], inv[14]];
        let (mut c, mut wsum) = ([0.0f32; 3], 0.0f32);
        for (v, weights) in ch.vweights.iter().enumerate() {
            for &(p, w) in weights {
                if p == pos && w > 0.01 {
                    c[0] += ch.verts[v * 3] * w;
                    c[1] += ch.verts[v * 3 + 1] * w;
                    c[2] += ch.verts[v * 3 + 2] * w;
                    wsum += w;
                }
            }
        }
        assert!(wsum > 0.0, "skin {pos} weights nothing");
        let cen = [c[0] / wsum, c[1] / wsum, c[2] / wsum];
        let d = ((origin[0] - cen[0]).powi(2)
            + (origin[1] - cen[1]).powi(2)
            + (origin[2] - cen[2]).powi(2))
        .sqrt();
        assert!(d < 0.35, "skin {pos} (bone {}) origin-centroid {d}", ch.bones[*bidx as usize].1);
    }
    // The reconstruction is exact: skin_verts at the
    // recovered bind reproduces the raw GEO verts.
    ch.reconstruct_bind();
    let globals = ch.compose_globals(&ch.bind_locals());
    let out = ch.skin_verts(&globals);
    let n = out.len() / 3;
    let mut s = 0.0f32;
    for i in 0..n * 3 {
        let d = out[i] - ch.verts[i];
        s += d * d;
    }
    let rms = (s / n as f32).sqrt();
    assert!(rms < 1e-3, "bind RMS {rms}");
}
