//! pose_test — animation pose skinning experiment.
//!
//! Pipeline under test (all pieces verified separately):
//!   skel_global_b  = compose GAO locals (pelvis local = identity)
//!   geo_global_b   = P * skel_global_b,  P = pelvis GAO global matrix
//!   skin matrix    = geo_global_b(posed) * bind_matrix_b (GEO skin)
//!   bone order     = wow record order of the B_<char> GAOs
//!                    (== GEO skin bone_idx order)
//! Modes: --bind-check | --render <combo> <out.png>
//!   combo = <mapping 1|2|3> x <convention 1|2|3>, or "bind".

use std::collections::HashMap;
use std::process::ExitCode;
use t2t::anim::{parse_pose_record, read_rot_keys};
use t2t::bigfile::BigFile;
use t2t::gao::{mat_mul, parse_gao, transform_point, IDENTITY};
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::png::write_png_rgba;
use t2t::wow::walk_wow;

type M = [f32; 16];

fn mat_inv(m: &M) -> Option<M> {
    let mut a = [[0f32; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            a[r][c] = m[c * 4 + r];
        }
        a[r][4 + r] = 1.0;
    }
    for col in 0..4 {
        let piv = (col..4).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        let d = a[col][col];
        for j in 0..8 {
            a[col][j] /= d;
        }
        for r in 0..4 {
            if r != col {
                let f = a[r][col];
                for j in 0..8 {
                    a[r][j] -= f * a[col][j];
                }
            }
        }
    }
    let mut out = [0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            out[c * 4 + r] = a[r][4 + c];
        }
    }
    Some(out)
}

fn quat_mat(q: [f32; 4]) -> M {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    // column-major rotation
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        0.0,
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        0.0,
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

fn with_t(mut m: M, t: [f32; 3]) -> M {
    m[12] = t[0];
    m[13] = t[1];
    m[14] = t[2];
    m
}

struct Ctx {
    /// Bone GAOs in record order: (key, name, father_key, local, gao_global)
    bones: Vec<(u32, String, u32, Option<M>, Option<M>)>,
    by_key: HashMap<u32, usize>,
    /// GEO vertices + skin (bone list position -> SkinBone)
    verts: Vec<f32>,
    faces: Vec<[u16; 7]>,
    uvs: Vec<f32>,
    /// per skin-list bone: (bone_idx = GAO order index, bind matrix)
    skin: Vec<(u16, M)>,
    /// per-vertex weights: list of (skin list position, weight)
    vweights: Vec<Vec<(usize, f32)>>,
    pose: Vec<t2t::anim::PoseUnit>,
    root_global: M,
}

fn load(bf_path: &str, wow_name: &str) -> Ctx {
    let bf = BigFile::open(bf_path).expect("archive");
    let entry = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(wow_name))
        .expect("entry");
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    let records = walk_wow(&data);

    let geo_key = std::env::var("T2T_GEO_KEY")
        .and_then(|s| u32::from_str_radix(&s, 16).map_err(|_| std::env::VarError::NotPresent))
        .unwrap_or(0x4200_35b0);
    let geo_rec = records.iter().find(|r| r.key == geo_key).expect("geo");
    let geo = parse_geometry(&data[geo_rec.data_range()]).expect("geo parse");
    let (_, skin_bones) = geo.skin.as_ref().expect("skin");
    let mut vweights: Vec<Vec<(usize, f32)>> = vec![Vec::new(); geo.nb_points()];
    let mut skin = Vec::new();
    for (pos, b) in skin_bones.iter().enumerate() {
        skin.push((b.bone_idx, b.bind_matrix));
        for &(v, w) in &b.weights {
            vweights[v as usize].push((pos, w as f32 / 65535.0));
        }
    }

    let mut bones = Vec::new();
    let mut by_key = HashMap::new();
    for r in &records {
        if r.tag == ".gao" {
            if let Some(g) = parse_gao(&data[r.data_range()]) {
                let prefix = std::env::var("T2T_BONE_PREFIX").unwrap_or_else(|_| "B_Kaileena".into());
                if g.name.starts_with(&prefix) {
                    by_key.insert(r.key, bones.len());
                    bones.push((r.key, g.name.clone(), g.father_key, g.local_matrix, g.global_matrix));
                }
            }
        }
    }
    let root_global = bones[0].4.expect("root global");

    let pose_key = std::env::var("T2T_POSE_KEY")
        .and_then(|s| u32::from_str_radix(&s, 16).map_err(|_| std::env::VarError::NotPresent))
        .unwrap_or(0x4200_359f);
    let pose = records
        .iter()
        .find(|r| r.key == pose_key)
        .and_then(|r| parse_pose_record(&data[r.data_range()]))
        .unwrap_or_default();

    Ctx {
        bones,
        by_key,
        verts: geo.vertices.clone(),
        faces: geo.faces.clone(),
        uvs: geo.uvs.clone(),
        skin,
        vweights,
        pose,
        root_global,
    }
}

/// Compose skeleton globals from per-bone local matrices.
fn compose_globals(ctx: &Ctx, locals: &[M]) -> Vec<M> {
    fn rec(
        i: usize,
        ctx: &Ctx,
        locals: &[M],
        cache: &mut HashMap<usize, M>,
    ) -> M {
        if let Some(m) = cache.get(&i) {
            return *m;
        }
        let father = ctx.bones[i].2;
        let m = if father == 0 {
            locals[i]
        } else if let Some(&fi) = ctx.by_key.get(&father) {
            let pg = rec(fi, ctx, locals, cache);
            mat_mul(&pg, &locals[i])
        } else {
            locals[i]
        };
        cache.insert(i, m);
        m
    }
    let mut cache = HashMap::new();
    (0..ctx.bones.len()).map(|i| rec(i, ctx, locals, &mut cache)).collect()
}

fn bind_locals(ctx: &Ctx) -> Vec<M> {
    ctx.bones
        .iter()
        .map(|b| b.3.unwrap_or(IDENTITY))
        .collect()
}

/// Skin all vertices with the given per-bone (GAO order) globals.
fn skin_verts(ctx: &Ctx, skel_globals: &[M]) -> Vec<f32> {
    // skin matrix per skin-list position
    let mut smats: Vec<M> = Vec::with_capacity(ctx.skin.len());
    for (bone_idx, bind) in &ctx.skin {
        let g = skel_globals[*bone_idx as usize];
        let geo_g = mat_mul(&ctx.root_global, &g);
        smats.push(mat_mul(&geo_g, bind));
    }
    let mut out = vec![0f32; ctx.verts.len()];
    for v in 0..ctx.verts.len() / 3 {
        let p = [ctx.verts[3 * v], ctx.verts[3 * v + 1], ctx.verts[3 * v + 2]];
        let mut acc = [0f32; 3];
        let mut wsum = 0f32;
        for &(pos, w) in &ctx.vweights[v] {
            let tp = transform_point(&smats[pos], p);
            acc[0] += w * tp[0];
            acc[1] += w * tp[1];
            acc[2] += w * tp[2];
            wsum += w;
        }
        if wsum > 0.0 {
            // Jade MRM raw weights sum to 0.25/0.5/0.75 per vertex
            // (never 1.0) — normalize so the blend is a proper
            // weighted average; exact for the bind pose.
            out[3 * v] = acc[0] / wsum;
            out[3 * v + 1] = acc[1] / wsum;
            out[3 * v + 2] = acc[2] / wsum;
        } else {
            out[3 * v] = p[0];
            out[3 * v + 1] = p[1];
            out[3 * v + 2] = p[2];
        }
    }
    out
}

/// Tiny flat-shaded renderer (front-ish view, Jade Z-up).
fn render(verts: &[f32], faces: &[[u16; 7]], w: u32, h: u32) -> Vec<u8> {
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for v in verts.chunks_exact(3) {
        for i in 0..3 {
            mn[i] = mn[i].min(v[i]);
            mx[i] = mx[i].max(v[i]);
        }
    }
    let c = [
        (mn[0] + mx[0]) / 2.0,
        (mn[1] + mx[1]) / 2.0,
        (mn[2] + mx[2]) / 2.0,
    ];
    let ext = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]).max(0.1);
    // camera at azim 35, elev 10, looking at center; Jade Z-up
    let (az, el) = (35f32.to_radians(), 10f32.to_radians());
    let dist = ext * 1.6;
    let eye = [
        c[0] + dist * el.cos() * az.sin(),
        c[1] - dist * el.cos() * az.cos(),
        c[2] + dist * el.sin(),
    ];
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let norm = |a: [f32; 3]| {
        let l = dot(a, a).sqrt().max(1e-9);
        [a[0] / l, a[1] / l, a[2] / l]
    };
    let fwd = norm(sub(c, eye));
    let right = norm(cross(fwd, [0.0, 0.0, 1.0]));
    let up = cross(right, fwd);
    let tan = 27.5f32.to_radians().tan();
    let aspect = w as f32 / h as f32;
    let proj = |p: [f32; 3]| -> Option<(f32, f32, f32)> {
        let d = sub(p, eye);
        let z = dot(d, fwd);
        if z <= ext * 0.01 {
            return None;
        }
        let x = dot(d, right) / (z * tan * aspect);
        let y = dot(d, up) / (z * tan);
        Some(((x * 0.5 + 0.5) * w as f32, (1.0 - (y * 0.5 + 0.5)) * h as f32, z))
    };
    let mut img = vec![0u8; (w * h * 4) as usize];
    for p in 0..(w * h) as usize {
        img[4 * p] = 16;
        img[4 * p + 1] = 18;
        img[4 * p + 2] = 26;
        img[4 * p + 3] = 255;
    }
    let mut zbuf = vec![f32::INFINITY; (w * h) as usize];
    let light = norm([0.35, -0.45, 0.82]);
    for f in faces {
        let pts: Vec<[f32; 3]> = (0..3)
            .map(|k| {
                let i = f[k] as usize;
                [verts[3 * i], verts[3 * i + 1], verts[3 * i + 2]]
            })
            .collect();
        let (Some(a), Some(b), Some(cc)) = (proj(pts[0]), proj(pts[1]), proj(pts[2])) else {
            continue;
        };
        let n = norm(cross(sub(pts[1], pts[0]), sub(pts[2], pts[0])));
        let shade = 0.3 + 0.7 * dot(n, light).abs();
        let (x0, y0) = (a.0, a.1);
        let (x1, y1) = (b.0, b.1);
        let (x2, y2) = (cc.0, cc.1);
        let minx = x0.min(x1).min(x2).max(0.0) as i32;
        let maxx = x0.max(x1).max(x2).min(w as f32 - 1.0) as i32;
        let miny = y0.min(y1).min(y2).max(0.0) as i32;
        let maxy = y0.max(y1).max(y2).min(h as f32 - 1.0) as i32;
        let area = (x1 - x0) * (y2 - y0) - (y1 - y0) * (x2 - x0);
        if area.abs() < 1e-9 {
            continue;
        }
        for y in miny..=maxy {
            for x in minx..=maxx {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = ((x1 - px) * (y2 - py) - (y1 - py) * (x2 - px)) / area;
                let w1 = ((x2 - px) * (y0 - py) - (y2 - py) * (x0 - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let iz = w0 / a.2 + w1 / b.2 + w2 / cc.2;
                let z = 1.0 / iz;
                let idx = y as usize * w as usize + x as usize;
                if z >= zbuf[idx] {
                    continue;
                }
                zbuf[idx] = z;
                let o = idx * 4;
                let g = (150.0 * shade) as u8;
                img[o] = g;
                img[o + 1] = (140.0 * shade) as u8;
                img[o + 2] = (125.0 * shade) as u8;
                img[o + 3] = 255;
            }
        }
    }
    img
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let ctx = load(&args[1], &args[2]);
    let mode = args.get(3).map(|s| s.as_str()).unwrap_or("--bind-check");

    if mode == "--weight-check" {
        let mut sums: HashMap<usize, u32> = HashMap::new();
        for (pos, _) in ctx.skin.iter().enumerate() {
            let _ = pos;
        }
        // rebuild raw sums from vweights (normalized) is lossy; use f32 sums
        let mut fsums: Vec<f32> = vec![0.0; ctx.verts.len() / 3];
        for (v, ws) in ctx.vweights.iter().enumerate() {
            fsums[v] = ws.iter().map(|w| w.1).sum();
        }
        let mut buckets: HashMap<String, usize> = HashMap::new();
        for s in &fsums {
            let k = format!("{:.2}", (s * 20.0).round() / 20.0);
            *buckets.entry(k).or_default() += 1;
        }
        let mut b: Vec<_> = buckets.into_iter().collect();
        b.sort();
        println!("per-vertex weight-sum histogram: {b:?}");
        let _ = sums;
        return ExitCode::SUCCESS;
    }

    if mode == "--bind-check" {
        // geo_global(bind) * bind_matrix must be identity per bone.
        let globals = compose_globals(&ctx, &bind_locals(&ctx));
        let mut worst = 0f32;
        for (bone_idx, bind) in &ctx.skin {
            let geo_g = mat_mul(&ctx.root_global, &globals[*bone_idx as usize]);
            let s = mat_mul(&geo_g, bind);
            let mut bw = 0f32;
            for (i, v) in s.iter().enumerate() {
                if i == 15 {
                    continue; // Jade matrices carry ~0 in m[15]
                }
                let want = if i % 5 == 0 { 1.0 } else { 0.0 };
                bw = bw.max((v - want).abs());
            }
            worst = worst.max(bw);
            let inv = mat_inv(bind).unwrap();
            println!(
                "bone_idx {:2} ({:<28}) worst {bw:.4}  composed_t [{:.3},{:.3},{:.3}] invbind_t [{:.3},{:.3},{:.3}]",
                bone_idx,
                ctx.bones[*bone_idx as usize].1,
                geo_g[12], geo_g[13], geo_g[14],
                inv[12], inv[13], inv[14]
            );
        }
        println!("bind check: worst |S - I| component = {worst:.6}");
        let pelvis = mat_mul(&ctx.root_global, &globals[0]);
        let s0 = mat_mul(&pelvis, &ctx.skin.iter().find(|s| s.0 == 0).unwrap().1);
        println!("root_global = {:?}", ctx.root_global);
        println!("S(pelvis) = geo_g * bind = {s0:?}");
        let spine = mat_mul(&ctx.root_global, &globals[1]);
        let s1 = mat_mul(&spine, &ctx.skin.iter().find(|s| s.0 == 1).unwrap().1);
        println!("S(spine) = {s1:?}");
        return ExitCode::SUCCESS;
    }

    if mode == "--render" {
        // args: --render <combo> <out.png>; combo "bind" or e.g. "m1c2"
        let combo = args.get(4).map(|s| s.as_str()).unwrap_or("bind");
        let out = args.get(5).expect("out.png");
        let locals = if let Some(fs) = combo.strip_prefix("anim") {
            // anim<F>: full multi-track pose of a clip at frame
            // F. Optional args after out.png: clip key hex
            // (default 420035a0), header key hex (default
            // 42000419). Channels map to bones via the header's
            // (i, v, x) table (channel = i-1, bone_idx = v-1,
            // 0xff = unmapped).
            let frame: u32 = fs.parse().unwrap_or(0);
            let clip_key = args
                .get(6)
                .and_then(|s| u32::from_str_radix(s, 16).ok())
                .unwrap_or(0x420035a0);
            let hdr_key = args
                .get(7)
                .and_then(|s| u32::from_str_radix(s, 16).ok())
                .unwrap_or(0x42000419);
            let bf = BigFile::open(&args[1]).expect("bf");
            let e = bf
                .active_files()
                .into_iter()
                .find(|e| e.name.contains(&args[2]))
                .expect("wow entry");
            let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
            let recs = walk_wow(&data);
            let clip = recs.iter().find(|r| r.key == clip_key).expect("clip");
            let cd = &data[clip.data_range()];
            let mut chan2bone: std::collections::HashMap<usize, usize> = Default::default();
            if std::env::var("ANIM_NAME_MAP").is_ok() {
                // Shared-bip hypothesis: Kaileena's header pairs,
                // translated by bone NAME (her custom rig's map
                // expressed in the standard bip layout).
                let pairs: [(usize, &str); 8] = [
                    (0, "Spine1"),
                    (9, "Neck"),
                    (10, "Head"),
                    (11, "Pelvis"),
                    (13, "L Clavicle"),
                    (15, "L UpperArm"),
                    (23, "R Clavicle"),
                    (25, "R UpperArm"),
                ];
                for (chan, name) in pairs {
                    if let Some((idx, _)) = ctx
                        .bones
                        .iter()
                        .enumerate()
                        .find(|(_, b)| b.1.ends_with(&format!(" {name}.gao")))
                    {
                        chan2bone.insert(chan, idx);
                    }
                }
            } else if std::env::var("ANIM_IDENTITY_MAP").is_ok() {
                // No header: channel i -> bone_idx i (record order).
                for i in 0..64 {
                    chan2bone.insert(i, i);
                }
            } else {
                let hdr = recs.iter().find(|r| r.key == hdr_key).expect("header");
                let hd = &data[hdr.data_range()];
                for t in hd.chunks_exact(3) {
                    if t[1] != 0xff {
                        chan2bone.insert((t[0] - 1) as usize, (t[1] - 1) as usize);
                    }
                }
            }
            // Walk sections with the generalized scanner.
            let mut rot: std::collections::HashMap<usize, Vec<(u16, [f32; 4])>> = Default::default();
            let mut trn: std::collections::HashMap<usize, Vec<(u16, [f32; 3])>> = Default::default();
            for sec in t2t::anim::parse_clip(cd) {
                match sec.role {
                    t2t::anim::SectionRole::Rot | t2t::anim::SectionRole::Rot5 => {
                        let keys = if sec.role == t2t::anim::SectionRole::Rot {
                            read_rot_keys(cd, sec.offset + 18, sec.count as usize)
                        } else {
                            t2t::anim::read_rot5_keys(cd, sec.offset + 18, sec.count as usize)
                        }
                        .expect("rot keys");
                        rot.insert(
                            sec.chan,
                            keys.iter().map(|k| (k.delta, k.rotation)).collect(),
                        );
                    }
                    t2t::anim::SectionRole::Trans => {
                        let keys = t2t::anim::read_trans_keys(cd, sec.offset + 18, sec.count as usize)
                            .expect("trans keys");
                        trn.insert(
                            sec.chan,
                            keys.iter().map(|k| (k.delta, k.value)).collect(),
                        );
                    }
                    _ => {}
                }
            }
            let mut locals = bind_locals(&ctx);
            eprintln!("anim debug: {} rot channels, {} mapped", rot.len(), chan2bone.len());
            let only_chan: Option<usize> = std::env::var("ANIM_ONLY_CHAN")
                .ok()
                .and_then(|s| s.parse().ok());
            for (chan, bone) in &chan2bone {
                if let Some(oc) = only_chan {
                    if *chan != oc {
                        continue;
                    }
                }
                let Some(keys) = rot.get(chan) else { continue };
                // Sample rotation at `frame` (slot 0 = frame 0).
                let mut t = 0u32;
                let mut cur = keys[0].1;
                for (i, (dl, q)) in keys.iter().enumerate() {
                    if i > 0 {
                        t += *dl as u32;
                    }
                    if t > frame {
                        break;
                    }
                    cur = *q;
                }
                let bind = locals[*bone];
                let trans = if std::env::var("ANIM_APPLY_TRANS").is_ok() {
                    if let Some(tv) = trn.get(chan) {
                        let mut t2 = 0u32;
                        let mut cur2 = tv[0].1;
                        for (i, (dl, v)) in tv.iter().enumerate() {
                            if i > 0 {
                                t2 += *dl as u32;
                            }
                            if t2 > frame {
                                break;
                            }
                            cur2 = *v;
                        }
                        cur2
                    } else {
                        [bind[12], bind[13], bind[14]]
                    }
                } else {
                    [bind[12], bind[13], bind[14]]
                };
                locals[*bone] = with_t(quat_mat(cur), trans);
            }
            locals
        } else if combo == "bind" {
            bind_locals(&ctx)
        } else if let Some(ks) = combo.strip_prefix("clip") {
            // clip<K>: rotation key K of clip 0x420035a0's stream
            // (253 keys at data offset 312) applied to bone_idx 2
            // (header 0x42000419 track 1 -> bone 3, 1-based),
            // convention 2: replace rotation, keep bind translation.
            let (k, bone) = match ks.split_once('b') {
                Some((a, b)) => (a.parse().unwrap_or(0), b.parse().unwrap_or(2)),
                None => (ks.parse().unwrap_or(0), 2),
            };
            let bf = BigFile::open(&args[1]).expect("bf");
            let e = bf
                .active_files()
                .into_iter()
                .find(|e| e.name.contains(&args[2]))
                .expect("wow entry");
            let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
            let recs = walk_wow(&data);
            let rec = recs.iter().find(|r| r.key == 0x420035a0).expect("clip record");
            let cdata = &data[rec.data_range()];
            let keys = read_rot_keys(cdata, 312, 253).expect("keys");
            let key = keys[k.min(252)];
            let mut locals = bind_locals(&ctx);
            let bind = locals[bone];
            locals[bone] = with_t(
                quat_mat(key.rotation),
                [bind[12], bind[13], bind[14]],
            );
            locals
        } else {
            let mapping: usize = combo[1..2].parse().unwrap_or(1);
            let conv: usize = combo[3..4].parse().unwrap_or(1);
            let mut locals = bind_locals(&ctx);
            // Build unit -> bone (GAO order position) assignment.
            let mut assign: Vec<Option<usize>> = vec![None; ctx.pose.len()];
            match mapping {
                4 => {
                    // header 0x42000313 table: unit index i is
                    // channel i; entry (i+1) gives bone v (1-based).
                    let bf = BigFile::open(&args[1]).expect("bf");
                    let e = bf
                        .active_files()
                        .into_iter()
                        .find(|e| e.name.contains(&args[2]))
                        .expect("wow entry");
                    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
                    let recs = walk_wow(&data);
                    let hdr = recs.iter().find(|r| r.key == 0x42000313).expect("header 313");
                    let hd = &data[hdr.data_range()];
                    let table: Vec<u8> = hd.chunks_exact(3).map(|t| t[1]).collect();
                    for (ui, a) in assign.iter_mut().enumerate() {
                        let idx = ctx.pose[ui].index;
                        if idx == u16::MAX {
                            continue; // root unit
                        }
                        if let Some(&v) = table.get(idx as usize) {
                            if v != 0xff {
                                *a = Some((v - 1) as usize);
                            }
                        }
                    }
                }
                1 => {
                    for (i, a) in assign.iter_mut().enumerate() {
                        *a = Some(i);
                    }
                }
                2 => {
                    // core bones only (skip MOD-prefixed), in record order
                    let core: Vec<usize> = (0..ctx.bones.len())
                        .filter(|&i| !ctx.bones[i].1.contains("MOD"))
                        .collect();
                    for (i, a) in assign.iter_mut().enumerate() {
                        *a = core.get(i).copied();
                    }
                }
                _ => {
                    // skin order: unit i -> skin list position i's bone_idx
                    for (i, a) in assign.iter_mut().enumerate() {
                        *a = ctx.skin.get(i).map(|s| s.0 as usize);
                    }
                }
            }
            for (ui, bpos) in assign.iter().enumerate() {
                let Some(bp) = *bpos else { continue };
                if bp >= locals.len() {
                    continue;
                }
                let u = &ctx.pose[ui];
                let bind = locals[bp];
                let rq = quat_mat(u.rotation);
                locals[bp] = match conv {
                    1 => with_t(rq, u.translation),
                    2 => with_t(rq, [bind[12], bind[13], bind[14]]),
                    _ => {
                        // delta: bind_R * pose_R, keep bind translation
                        let mut d = mat_mul(&bind, &rq);
                        d[12] = bind[12];
                        d[13] = bind[13];
                        d[14] = bind[14];
                        d
                    }
                };
            }
            locals
        };
        let globals = compose_globals(&ctx, &locals);
        let posed = skin_verts(&ctx, &globals);
        let (mut mn, mut mx) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for v in posed.chunks_exact(3) {
            for i in 0..3 {
                mn[i] = mn[i].min(v[i]);
                mx[i] = mx[i].max(v[i]);
            }
        }
        println!("combo {combo}: posed AABB [{:.2},{:.2},{:.2}]..[{:.2},{:.2},{:.2}]", mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]);
        let img = render(&posed, &ctx.faces, 360, 300);
        match write_png_rgba(std::path::Path::new(out), 360, 300, &img) {
            Ok(()) => println!("saved {out}"),
            Err(e) => eprintln!("write failed: {e}"),
        }
        return ExitCode::SUCCESS;
    }
    ExitCode::SUCCESS
}
