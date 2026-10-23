//! Clothed-Prince bind hunt: the clothed GEO 0x6d002617
//! lives only in PrinceLoading_Shape, whose B_Pr GAOs
//! are stored posed. Test the hybrid: clothed GEO +
//! PrinceFinal's B_Pr bone matrices (transplanted by
//! bone key). A true bind rig reproduces the GEO's raw
//! vertices exactly through skin_verts at bind locals.
use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::png::write_png_rgba;
use t2t::skin::Character;

fn sector(bf: &BigFile, substr: &str) -> Vec<u8> {
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(substr))
        .unwrap_or_else(|| panic!("sector {substr}"));
    decompress_entry(bf.read_data(e.index)).expect("decompress")
}

fn bind_rms(ch: &Character) -> f32 {
    let locals = ch.bind_locals();
    let globals = ch.compose_globals(&locals);
    let out = ch.skin_verts(&globals);
    let n = out.len() / 3;
    let mut s = 0.0f32;
    for i in 0..n * 3 {
        let d = out[i] - ch.verts[i];
        s += d * d;
    }
    (s / n as f32).sqrt()
}

fn render(ch: &Character, path: &str) {
    let locals = ch.bind_locals();
    let globals = ch.compose_globals(&locals);
    let verts = ch.skin_verts(&globals);
    // Auto-fit the projection to the posed AABB.
    let (mut mn, mut mx) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in verts.chunks_exact(3) {
        for i in 0..3 {
            mn[i] = mn[i].min(p[i]);
            mx[i] = mx[i].max(p[i]);
        }
    }
    println!("posed AABB: {mn:?} .. {mx:?}");
    let (w, h) = (440usize, 600usize);
    let mut fb = vec![[24u8, 24, 28, 255]; w * h];
    let mut zb = vec![f32::NEG_INFINITY; w * h];
    for f in &ch.faces {
        let pts: Vec<[f32; 3]> = (0..3)
            .map(|k| {
                let o = f[k] as usize * 3;
                [verts[o], verts[o + 1], verts[o + 2]]
            })
            .collect();
        let (cx, cz) = ((mn[0] + mx[0]) / 2.0, (mn[2] + mx[2]) / 2.0);
        let span = (mx[0] - mn[0]).max(mx[2] - mn[2]) * 1.15;
        let proj = |p: [f32; 3]| -> [f32; 3] {
            [
                ((p[0] - cx) / span + 0.5) * w as f32,
                (1.0 - ((p[2] - cz) / span + 0.5)) * h as f32,
                p[1],
            ]
        };
        let (pa, pb, pc) = (proj(pts[0]), proj(pts[1]), proj(pts[2]));
        let e1 = [pb[0] - pa[0], pb[1] - pa[1]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1]];
        let nz = (e1[0] * e2[1] - e1[1] * e2[0]).abs();
        let shade = (140.0 + nz * 2.0).min(235.0) as u8;
        let (mn_x, mx_x) = (
            pa[0].min(pb[0]).min(pc[0]).max(0.0),
            pa[0].max(pb[0]).max(pc[0]).min(w as f32 - 1.0),
        );
        let (mn_y, mx_y) = (
            pa[1].min(pb[1]).min(pc[1]).max(0.0),
            pa[1].max(pb[1]).max(pc[1]).min(h as f32 - 1.0),
        );
        let denom = (pb[1] - pc[1]) * (pa[0] - pc[0]) + (pc[0] - pb[0]) * (pa[1] - pc[1]);
        if denom.abs() < 1e-9 {
            continue;
        }
        for y in mn_y as usize..=mx_y as usize {
            for x in mn_x as usize..=mx_x as usize {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((pb[1] - pc[1]) * (px - pc[0]) + (pc[0] - pb[0]) * (py - pc[1])) / denom;
                let w1 = ((pc[1] - pa[1]) * (px - pc[0]) + (pa[0] - pc[0]) * (py - pc[1])) / denom;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let d = w0 * pa[2] + w1 * pb[2] + w2 * pc[2];
                let idx = y * w + x;
                if d > zb[idx] {
                    zb[idx] = d;
                    fb[idx] = [shade, shade, shade, 255];
                }
            }
        }
    }
    let flat: Vec<u8> = fb.iter().flat_map(|p| [p[0], p[1], p[2], p[3]]).collect();
    write_png_rgba(std::path::Path::new(path), w as u32, h as u32, &flat).expect("png");
    println!("wrote {path}");
}

fn dev(m: &[f32; 16]) -> f32 {
    let mut mx = 0.0f32;
    for r in 0..3 {
        for c in 0..4 {
            let want = if r == c { 1.0 } else { 0.0 };
            mx = mx.max((m[r * 4 + c] - want).abs());
        }
    }
    mx
}

/// prince_diag-style self-deviation summary under a
/// chosen root bridge and global source.
fn diag(ch: &Character, root: [f32; 16], stored: bool, label: &str) {
    let globals: Vec<[f32; 16]> = if stored {
        ch.bones.iter().map(|b| b.4.unwrap_or(t2t::gao::IDENTITY)).collect()
    } else {
        ch.compose_globals(&ch.bind_locals())
    };
    let geo_globals: Vec<[f32; 16]> = globals
        .iter()
        .map(|g| t2t::gao::mat_mul(&root, g))
        .collect();
    let mut devs: Vec<f32> = Vec::new();
    for (bidx, bind) in &ch.skin {
        let s = t2t::gao::mat_mul(&geo_globals[*bidx as usize], bind);
        devs.push(dev(&s));
    }
    devs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let good = devs.iter().filter(|d| **d < 0.05).count();
    println!(
        "{label}: skin self-dev median {:.4} max {:.4}, {}/{} bones < 0.05",
        devs[devs.len() / 2],
        devs.last().copied().unwrap_or(0.0),
        good,
        devs.len()
    );
}

/// General 4x4 inverse (Gauss-Jordan with pivoting).
fn mat_inv(m: &[f32; 16]) -> Option<[f32; 16]> {
    let mut a = [[0.0f32; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            a[r][c] = m[c * 4 + r]; // to row-major
            a[r][c + 4] = if r == c { 1.0 } else { 0.0 };
        }
    }
    for col in 0..4 {
        let mut piv = col;
        for r in col + 1..4 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        if a[piv][col].abs() < 1e-9 {
            return None;
        }
        a.swap(col, piv);
        let d = a[col][col];
        for c in 0..8 {
            a[col][c] /= d;
        }
        for r in 0..4 {
            if r != col {
                let f = a[r][col];
                if f != 0.0 {
                    for c in 0..8 {
                        a[r][c] -= f * a[col][c];
                    }
                }
            }
        }
    }
    let mut out = [0.0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            out[c * 4 + r] = a[r][c + 4];
        }
    }
    Some(out)
}

/// Reconstruct bind globals from the GEO's bind
/// matrices: at bind, (root * G_b) * B_b = I; with the
/// root taken as identity, G_b = inverse(B_b). Locals
/// are then derived from the globals via the father
/// chain. Non-skin bones keep their composed globals
/// (they do not deform this GEO).
fn reconstruct(ch: &mut Character) {
    let composed = ch.compose_globals(&ch.bind_locals());
    let mut globals: Vec<[f32; 16]> = composed.clone();
    for (bidx, bind) in &ch.skin {
        if let Some(inv) = mat_inv(bind) {
            globals[*bidx as usize] = inv;
        }
    }
    let inv_globals: Vec<Option<[f32; 16]>> = globals.iter().map(|g| mat_inv(g)).collect();
    for i in 0..ch.bones.len() {
        let father = ch.bones[i].2;
        let local = if let Some(&fi) = ch.by_key.get(&father) {
            match inv_globals[fi] {
                Some(inv) => t2t::gao::mat_mul(&inv, &globals[i]),
                None => globals[i],
            }
        } else {
            globals[i]
        };
        ch.bones[i].3 = Some(local);
        ch.bones[i].4 = Some(globals[i]);
    }
    ch.root_global = t2t::gao::IDENTITY;
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let loading = sector(&bf, "PrinceLoading_Shape");
    let fin = sector(&bf, "PrinceFinal_Shape");
    let mut ch = Character::load(&loading, 0x6d002617, "B_Pr").expect("clothed load");
    let chf = Character::load(&fin, 0x510022d3, "B_Pr").expect("bare load");
    println!(
        "clothed: {} bones collected, {} skin bones; bare: {} bones",
        ch.bones.len(),
        ch.skin.len(),
        chf.bones.len()
    );
    println!("bind RMS as stored (posed rig): {:.4}", bind_rms(&ch));
    println!("bind RMS bare (sanity): {:.4}", bind_rms(&chf));
    // Root-bridge variants: Character::load takes
    // bones.first() as root — here that is Face Neck,
    // not the Pelvis. Try the Pelvis bone's own global.
    let pelvis = ch
        .bones
        .iter()
        .position(|b| b.1 == "B_Pr_Bip Pelvis.gao")
        .expect("pelvis");
    let mut ch2 = Character::load(&loading, 0x6d002617, "B_Pr").expect("reload");
    ch2.root_global = ch2.bones[pelvis].4.expect("pelvis global");
    println!("bind RMS with pelvis root (stored global): {:.4}", bind_rms(&ch2));
    let mut ch3 = Character::load(&loading, 0x6d002617, "B_Pr").expect("reload");
    let comp = ch3.compose_globals(&ch3.bind_locals());
    ch3.root_global = comp[pelvis];
    println!("bind RMS with pelvis root (composed): {:.4}", bind_rms(&ch3));
    // Key-order comparison of the two collections.
    let mut diffs = 0;
    for (i, b) in ch.bones.iter().enumerate() {
        match chf.bones.get(i) {
            Some(fb) if fb.0 == b.0 => {}
            Some(fb) => {
                if diffs < 5 {
                    println!("order differs at {i}: loading {:#010x} {} vs final {:#010x} {}", b.0, b.1, fb.0, fb.1);
                }
                diffs += 1;
            }
            None => {
                if diffs < 5 {
                    println!("loading has extra bone at {i}: {:#010x} {}", b.0, b.1);
                }
                diffs += 1;
            }
        }
    }
    println!("collection diffs: {diffs}");
    // Transplant by NAME (keys are sector-local; the
    // skeleton is the same). Locals are parent-relative,
    // so they carry across sectors; globals re-derive.
    let by_name: std::collections::HashMap<&str, usize> = chf
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| (b.1.as_str(), i))
        .collect();
    let mut matched = 0;
    let mut unmatched: Vec<String> = Vec::new();
    for b in ch.bones.iter_mut() {
        if let Some(&fi) = by_name.get(b.1.as_str()) {
            b.3 = chf.bones[fi].3;
            b.4 = chf.bones[fi].4;
            matched += 1;
        } else {
            unmatched.push(b.1.clone());
        }
    }
    ch.root_global = chf.root_global;
    println!("transplanted {matched} bones; unmatched: {:?}", unmatched);
    println!("bind RMS hybrid: {:.4}", bind_rms(&ch));
    render(&ch, "out/clothed_bind_hybrid.png");
    // Root/source sweep on the as-stored clothed rig.
    let chl = Character::load(&loading, 0x6d002617, "B_Pr").expect("reload");
    let pelvis_i = chl
        .bones
        .iter()
        .position(|b| b.1 == "B_Pr_Bip Pelvis.gao")
        .expect("pelvis");
    let comp = chl.compose_globals(&chl.bind_locals());
    diag(&chl, chl.root_global, false, "root=first stored, composed globals");
    diag(&chl, chl.bones[pelvis_i].4.unwrap(), false, "root=pelvis stored, composed globals");
    diag(&chl, comp[pelvis_i], false, "root=pelvis composed, composed globals");
    diag(&chl, t2t::gao::IDENTITY, false, "root=identity, composed globals");
    diag(&chl, chl.root_global, true, "root=first stored, STORED globals");
    diag(&chl, t2t::gao::IDENTITY, true, "root=identity, STORED globals");
    // Control: the bare rig under the same sweep.
    diag(&chf, chf.root_global, false, "BARE root=first stored, composed");
    diag(&chf, t2t::gao::IDENTITY, true, "BARE root=identity, STORED");
    // Father-structure comparison (loading vs final).
    let father_name = |ch: &Character, i: usize| -> String {
        let f = ch.bones[i].2;
        ch.by_key
            .get(&f)
            .map(|&fi| ch.bones[fi].1.clone())
            .unwrap_or_else(|| format!("<{f:#x}>"))
    };
    let fin_by_name: std::collections::HashMap<&str, usize> = chf
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| (b.1.as_str(), i))
        .collect();
    for (i, b) in chl.bones.iter().enumerate() {
        if let Some(&fi) = fin_by_name.get(b.1.as_str()) {
            let (fl, ff) = (father_name(&chl, i), father_name(&chf, fi));
            if fl != ff {
                println!("father differs: {} loading-father {} vs final-father {}", b.1, fl, ff);
            }
        }
    }
    // Root-constancy test: if the clothed GEO was
    // authored against the same canonical skeleton as
    // the bare GEO, then root_b = B_b^-1 * G_bare_b^-1
    // is the SAME matrix for every skin bone.
    {
        let bare_globals = chf.compose_globals(&chf.bind_locals());
        let bare_by_name: std::collections::HashMap<&str, [f32; 16]> = chf
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| (b.1.as_str(), bare_globals[i]))
            .collect();
        let mut roots: Vec<[f32; 16]> = Vec::new();
        for (bidx, bind) in &chl.skin {
            let name = chl.bones[*bidx as usize].1.as_str();
            if let (Some(inv_bind), Some(gb)) = (mat_inv(bind), bare_by_name.get(name)) {
                if let Some(inv_gb) = mat_inv(gb) {
                    roots.push(t2t::gao::mat_mul(&inv_bind, &inv_gb));
                }
            }
        }
        let ref_root = roots[0];
        let mut max_dev = 0.0f32;
        for r in &roots {
            for i in 0..16 {
                max_dev = max_dev.max((r[i] - ref_root[i]).abs());
            }
        }
        println!(
            "root-constancy: {} skin bones, max element deviation across root_b: {:.6}",
            roots.len(),
            max_dev
        );
        println!("reference root_b translation: [{:.3}, {:.3}, {:.3}]", ref_root[12], ref_root[13], ref_root[14]);
    }
    // Pairing test: the clothed skin list is reversed
    // (bone_idx counts down). Maybe the bind matrices
    // are stored in the opposite order to the indices:
    // re-pair entry i's bind with entry (n-1-i)'s bone
    // and check B^-1 origins along the spine chain.
    {
        let n = chl.skin.len();
        println!("pairing test (spine chain B^-1 origins):");
        for (label, flip) in [("as-parsed", false), ("flipped", true)] {
            let mut origins: Vec<(String, [f32; 3])> = Vec::new();
            for i in 0..n {
                let (bidx, bind) = chl.skin[i];
                let bidx = if flip { chl.skin[n - 1 - i].0 } else { bidx };
                if let Some(inv) = mat_inv(&bind) {
                    origins.push((
                        chl.bones[bidx as usize].1.clone(),
                        [inv[12], inv[13], inv[14]],
                    ));
                }
            }
            for want in ["B_Pr_Bip Pelvis.gao", "B_Pr_Bip Spine.gao", "B_Pr_Bip Head.gao", "B_Pr_Bip L Foot.gao"] {
                if let Some((_, o)) = origins.iter().find(|(nm, _)| nm == want) {
                    println!("  {label} {want}: [{:.3}, {:.3}, {:.3}]", o[0], o[1], o[2]);
                }
            }
        }
    }
    // Addressing test: does bone_idx address the FULL
    // GAO collection rather than the B_Pr-filtered one?
    // For each skin entry, compare the B^-1 origin with
    // the centroid of the verts it weights: a correct
    // pairing puts the origin among its own verts.
    {
        let full = Character::load(&loading, 0x6d002617, "").expect("full collection");
        println!(
            "full collection: {} GAOs; skin entries {}",
            full.bones.len(),
            full.skin.len()
        );
        for (label, chx) in [("B_Pr-filtered", &chl), ("full", &full)] {
            let mut dists: Vec<f32> = Vec::new();
            for (pos, (bidx, bind)) in chx.skin.iter().enumerate() {
                let Some(inv) = mat_inv(bind) else { continue };
                let origin = [inv[12], inv[13], inv[14]];
                // Centroid of verts weighted to this entry.
                let (mut c, mut wsum) = ([0.0f32; 3], 0.0f32);
                for (v, weights) in chx.vweights.iter().enumerate() {
                    for &(p, w) in weights {
                        if p == pos && w > 0.01 {
                            c[0] += chx.verts[v * 3] * w;
                            c[1] += chx.verts[v * 3 + 1] * w;
                            c[2] += chx.verts[v * 3 + 2] * w;
                            wsum += w;
                        }
                    }
                }
                if wsum > 0.0 {
                    let cen = [c[0] / wsum, c[1] / wsum, c[2] / wsum];
                    let d = ((origin[0] - cen[0]).powi(2)
                        + (origin[1] - cen[1]).powi(2)
                        + (origin[2] - cen[2]).powi(2))
                    .sqrt();
                    dists.push(d);
                    if dists.len() <= 4 {
                        println!(
                            "  {label} skin {pos} bone {} origin [{:.2},{:.2},{:.2}] centroid [{:.2},{:.2},{:.2}] d {d:.2}",
                            chx.bones[*bidx as usize].1, origin[0], origin[1], origin[2], cen[0], cen[1], cen[2]
                        );
                    }
                }
            }
            dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if !dists.is_empty() {
                println!(
                    "  {label}: origin-to-centroid distance median {:.3} max {:.3} over {} entries",
                    dists[dists.len() / 2],
                    dists.last().unwrap(),
                    dists.len()
                );
            }
        }
    }
    // Rotation comparison: are the clothed B^-1 bone
    // rotations the canonical ones (frame offset only)
    // or genuinely posed (per-bone rotation deltas)?
    {
        let bare_g = chf.compose_globals(&chf.bind_locals());
        let bare_by_name: std::collections::HashMap<&str, [f32; 16]> = chf
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| (b.1.as_str(), t2t::gao::mat_mul(&chf.root_global, &bare_g[i])))
            .collect();
        let rot_dev = |a: &[f32; 16], b: &[f32; 16]| -> f32 {
            let mut mx = 0.0f32;
            for i in [0usize, 1, 2, 4, 5, 6, 8, 9, 10] {
                mx = mx.max((a[i] - b[i]).abs());
            }
            mx
        };
        for (bidx, bind) in &chl.skin {
            let name = chl.bones[*bidx as usize].1.as_str();
            if name.contains("Bip") {
                if let (Some(inv), Some(bg)) = (mat_inv(bind), bare_by_name.get(name)) {
                    println!(
                        "  rot-dev {name}: {:.4} (B^-1 origin [{:.2},{:.2},{:.2}] vs bare bridged [{:.2},{:.2},{:.2}])",
                        rot_dev(&inv, bg),
                        inv[12], inv[13], inv[14], bg[12], bg[13], bg[14]
                    );
                }
            }
        }
    }
    // Reconstruction: bind globals from bind matrices.
    let mut chr = Character::load(&loading, 0x6d002617, "B_Pr").expect("reload");
    reconstruct(&mut chr);
    println!("bind RMS reconstructed: {:.6}", bind_rms(&chr));
    diag(&chr, chr.root_global, false, "RECONSTRUCTED root=identity, composed");
    render(&chr, "out/clothed_bind_reconstructed.png");
    ExitCode::SUCCESS
}
