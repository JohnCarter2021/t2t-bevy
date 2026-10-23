//! wow_render — assemble a _wow_ world (GAO transforms + materials)
//! and software-render a textured perspective preview to PNG.
//! Jade is Z-up: ground plane is XY, up is +Z.
//!
//! Usage: wow_render <archive.bf> <entry-index|name-substring> <out.png>
//!          [--width N] [--height N] [--azim deg] [--elev deg]

use std::collections::HashMap;
use std::process::ExitCode;
use t2t::assemble::{assemble, AssembledWorld};
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::png::write_png_rgba;
use t2t::texture::decode_texture;
use t2t::wow::walk_wow;

fn argf(args: &[String], name: &str) -> Option<f32> {
    args.iter()
        .position(|a| a == name)
        .and_then(|p| args.get(p + 1))
        .and_then(|s| s.parse().ok())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: wow_render <archive.bf> <entry|name> <out.png> [--width N] [--height N] [--azim deg] [--elev deg]");
        return ExitCode::from(2);
    }
    let bf = match BigFile::open(&args[1]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let entry = if let Ok(idx) = args[2].parse::<u32>() {
        bf.files.get(&idx)
    } else {
        bf.active_files().into_iter().find(|e| e.name.contains(&args[2]))
    };
    let Some(entry) = entry else {
        eprintln!("entry not found: {}", args[2]);
        return ExitCode::FAILURE;
    };
    let data = match decompress_entry(bf.read_data(entry.index)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("decompress failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let records = walk_wow(&data);
    let only_gao = args
        .iter()
        .position(|a| a == "--only-gao")
        .and_then(|p| args.get(p + 1).cloned());
    if args.iter().any(|a| a == "--dump-gao") {
        use t2t::gao::parse_gao;
        let mut mat_kinds: std::collections::HashMap<u32, usize> = Default::default();
        for r in &records {
            if let Some(g) = r.gro_type {
                if (3..=5).contains(&g) {
                    *mat_kinds.entry(g).or_default() += 1;
                }
            }
        }
        println!("material records by gro_type: {mat_kinds:?}");
        if let Some(p) = args.iter().position(|a| a == "--find-geo") {
            if let Some(ks) = args.get(p + 1) {
                let want = u32::from_str_radix(ks.trim_start_matches("0x"), 16).unwrap_or(0);
                let world0 = assemble(&data, &records);
                for pg in &world0.placed {
                    // placed geos keep no key; match by tris/points via gao list below
                    let _ = pg;
                }
                for r in &records {
                    if r.tag == ".gao" {
                        if let Some(g) = parse_gao(&data[r.data_range()]) {
                            if g.gro_key == want {
                                println!("GAO direct: {:?} gro 0x{:08x} grm 0x{:08x}", g.name, g.gro_key, g.grm_key);
                            }
                        }
                    }
                }
                // Group membership: scan every non-geo record for the key word.
                for r in &records {
                    let sub = &data[r.data_range()];
                    if sub.windows(4).any(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]) == want) && r.key != want {
                        println!("record 0x{:08x} (tag={} gro={:?}) references geo key", r.key, r.tag, r.gro_type);
                    }
                }
            }
            return ExitCode::SUCCESS;
        }
        // Debug the full chain for grm 0x8f001395.
        {
            use t2t::material::parse_material;
            let find = |k: u32| records.iter().find(|x| x.key == k);
            if let Some(mrec) = find(0x8f001395) {
                let mdata = &data[mrec.data_range()];
                println!("grm record size {} gro {:?}", mdata.len(), mrec.gro_type);
                if let Some(m) = parse_material(mdata, mrec.gro_type.unwrap_or(0)) {
                    println!("  multi kind {} subs: {:08x?}", m.kind, &m.sub_material_keys[..m.sub_material_keys.len().min(8)]);
                    for sk in m.sub_material_keys.iter().take(6) {
                        match find(*sk) {
                            Some(sr) => {
                                let sd = &data[sr.data_range()];
                                let sm = parse_material(sd, sr.gro_type.unwrap_or(99));
                                println!("    sub 0x{sk:08x} gro {:?} -> {:?}", sr.gro_type, sm.map(|x| (x.kind, x.texture_keys)));
                            }
                            None => println!("    sub 0x{sk:08x} NOT in wow", ),
                        }
                    }
                }
            }
            if let Some(sr) = find(0x8f001357) {
                let sd = &data[sr.data_range()];
                print!("sub 0x8f001357 raw u32s:");
                for o in (0..sd.len().min(96)).step_by(4) {
                    let v = u32::from_le_bytes([sd[o], sd[o + 1], sd[o + 2], sd[o + 3]]);
                    print!(" [{o}]={v:08x}");
                }
                println!(" (len {})", sd.len());
            }
            // Also: what matIds do geometries actually carry?
            let mut ids: std::collections::HashSet<u32> = Default::default();
            for r in &records {
                let sub = &data[r.data_range()];
                if let Ok(g) = t2t::geometry::parse_geometry(sub) {
                    for (_, mid) in &g.elements {
                        ids.insert(*mid);
                    }
                }
            }
            let mut ids: Vec<u32> = ids.into_iter().collect();
            ids.sort();
            println!("distinct GEO matIds: {ids:?}");
        }
        let mut shown = 0;
        for r in &records {
            if r.tag == ".gao" {
                let sub = &data[r.data_range()];
                if let Some(g) = parse_gao(sub) {
                    if g.has_visual && shown < 12 {
                        let grm_in = records.iter().find(|x| x.key == g.grm_key);
                        println!(
                            "GAO {:<28} gro 0x{:08x} grm 0x{:08x} (in-wow: {}) gmat_t {} name={:?}",
                            "",
                            g.gro_key,
                            g.grm_key,
                            grm_in
                                .map(|x| format!("yes gro={:?} tag={}", x.gro_type, x.tag))
                                .unwrap_or_else(|| "no".into()),
                            g.gmat_type,
                            g.name
                        );
                        shown += 1;
                    }
                }
            }
        }
        return ExitCode::SUCCESS;
    }
    let mut world = assemble(&data, &records);
    if args.iter().any(|a| a == "--local") {
        // Bind-pose inspection: ignore GAO placement (characters are
        // positioned by their skeleton at runtime; the stored GAO
        // matrix for skinned meshes is not their render transform).
        for p in &mut world.placed {
            p.world_vertices = p.geo.vertices.clone();
        }
    }
    if let Some(name) = &only_gao {
        if name.ends_with(".gao") {
            world.placed.retain(|p| p.gao_name == *name);
        } else {
            world.placed.retain(|p| p.gao_name.contains(name.as_str()));
        }
        println!("--only-gao {name}: {} placed geos remain", world.placed.len());
        for p in &world.placed {
            let (a, b) = p.geo.aabb();
            println!(
                "   placed {:?}: {} pts {} tris local AABB [{:.2},{:.2},{:.2}]..[{:.2},{:.2},{:.2}] skin={}",
                p.gao_name,
                p.geo.nb_points(),
                p.geo.nb_tris(),
                a[0], a[1], a[2], b[0], b[1], b[2],
                p.geo.skin.as_ref().map(|(_, bs)| bs.len()).unwrap_or(0)
            );
        }
    }
    let s = &world.stats;
    println!(
        "assembled {}: {} GAOs ({} visual), {} geos placed, {} tris",
        bf.file_path(entry),
        s.gaos,
        s.gaos_with_visual,
        s.geos_placed,
        world.total_tris()
    );
    println!(
        "elements {} ({} with texture key); textures: {} resolved to pixels, {} stub-only, {} missing",
        s.elements,
        s.elements_with_texture_key,
        s.texture_keys_resolved_to_pixels,
        s.texture_keys_stub_only,
        s.texture_keys_missing
    );
    let (mn, mx) = world.aabb();
    println!(
        "world AABB: [{:.1},{:.1},{:.1}] .. [{:.1},{:.1},{:.1}]",
        mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]
    );

    // Decode textures lazily into RGBA.
    let mut tex_cache: HashMap<u32, (u32, u32, Vec<u8>)> = HashMap::new();
    for (key, (ti, range)) in &world.textures {
        let sub = &data[range.clone()];
        let rgba = decode_texture(sub, ti);
        if rgba.len() == ti.width as usize * ti.height as usize * 4 && !rgba.is_empty() {
            tex_cache.insert(*key, (ti.width, ti.height, rgba));
        }
    }
    println!("{} textures decoded for render", tex_cache.len());

    let w = argf(&args, "--width").unwrap_or(1100.0) as u32;
    let h = argf(&args, "--height").unwrap_or(800.0) as u32;
    let azim = argf(&args, "--azim").unwrap_or(35.0).to_radians();
    let elev = argf(&args, "--elev").unwrap_or(28.0).to_radians();
    let dist_factor = argf(&args, "--dist").unwrap_or(1.45);
    let img = render(&world, &tex_cache, w, h, azim, elev, dist_factor);
    match write_png_rgba(std::path::Path::new(&args[3]), w, h, &img) {
        Ok(()) => println!("saved {}", args[3]),
        Err(e) => eprintln!("write failed: {e}"),
    }
    ExitCode::SUCCESS
}

struct Cam {
    eye: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    fwd: [f32; 3],
    tan_half: f32,
    aspect: f32,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-9);
    [a[0] / l, a[1] / l, a[2] / l]
}

fn render(
    world: &AssembledWorld,
    tex: &HashMap<u32, (u32, u32, Vec<u8>)>,
    w: u32,
    h: u32,
    azim: f32,
    elev: f32,
    dist_factor: f32,
) -> Vec<u8> {
    let (mn, mx) = world.aabb();
    let center = [
        (mn[0] + mx[0]) / 2.0,
        (mn[1] + mx[1]) / 2.0,
        (mn[2] + mx[2]) / 2.0,
    ];
    let extent = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]).max(1.0);
    let dist = extent * dist_factor;
    let eye = [
        center[0] + dist * elev.cos() * azim.sin(),
        center[1] - dist * elev.cos() * azim.cos(),
        center[2] + dist * elev.sin(),
    ];
    let fwd = norm(sub(center, eye));
    let right = norm(cross(fwd, [0.0, 0.0, 1.0]));
    let up = cross(right, fwd);
    let cam = Cam {
        eye,
        right,
        up,
        fwd,
        tan_half: (27.5f32.to_radians()).tan(),
        aspect: w as f32 / h as f32,
    };
    let light = norm([0.35, -0.45, 0.82]);

    let mut img = vec![0u8; (w * h * 4) as usize];
    for p in 0..(w * h) as usize {
        img[4 * p] = 18;
        img[4 * p + 1] = 20;
        img[4 * p + 2] = 28;
        img[4 * p + 3] = 255;
    }
    let mut zbuf = vec![f32::INFINITY; (w * h) as usize];

    let project = |p: [f32; 3]| -> Option<(f32, f32, f32)> {
        let d = sub(p, cam.eye);
        let z = dot(d, cam.fwd);
        if z <= extent * 0.01 {
            return None;
        }
        let x = dot(d, cam.right) / (z * cam.tan_half * cam.aspect);
        let y = dot(d, cam.up) / (z * cam.tan_half);
        Some((
            (x * 0.5 + 0.5) * w as f32,
            (1.0 - (y * 0.5 + 0.5)) * h as f32,
            z,
        ))
    };

    for placed in &world.placed {
        let geo = &placed.geo;
        let wv = &placed.world_vertices;
        for f in &geo.faces {
            let pi = [f[0] as usize, f[1] as usize, f[2] as usize];
            let pts: Vec<[f32; 3]> = pi
                .iter()
                .map(|&i| [wv[3 * i], wv[3 * i + 1], wv[3 * i + 2]])
                .collect();
            let (Some(a), Some(b), Some(c)) = (project(pts[0]), project(pts[1]), project(pts[2]))
            else {
                continue;
            };
            // Face normal + lambert shade.
            let n = norm(cross(sub(pts[1], pts[0]), sub(pts[2], pts[0])));
            let shade = 0.35 + 0.65 * dot(n, light).abs();
            let texkey = placed.element_textures.get(f[6] as usize).copied().flatten();
            let texture = texkey.and_then(|k| tex.get(&k));
            let uvs: [[f32; 2]; 3] = std::array::from_fn(|k| {
                let ui = f[3 + k] as usize * 2;
                if ui + 1 < geo.uvs.len() {
                    [geo.uvs[ui], geo.uvs[ui + 1]]
                } else {
                    [0.0, 0.0]
                }
            });
            rasterize(
                &mut img, &mut zbuf, w, h, [a, b, c], uvs, texture, shade,
            );
        }
    }
    img
}

#[allow(clippy::too_many_arguments)]
fn rasterize(
    img: &mut [u8],
    zbuf: &mut [f32],
    w: u32,
    h: u32,
    v: [(f32, f32, f32); 3],
    uvs: [[f32; 2]; 3],
    texture: Option<&(u32, u32, Vec<u8>)>,
    shade: f32,
) {
    let (x0, y0) = (v[0].0, v[0].1);
    let (x1, y1) = (v[1].0, v[1].1);
    let (x2, y2) = (v[2].0, v[2].1);
    let minx = x0.min(x1).min(x2).max(0.0) as i32;
    let maxx = x0.max(x1).max(x2).min(w as f32 - 1.0) as i32;
    let miny = y0.min(y1).min(y2).max(0.0) as i32;
    let maxy = y0.max(y1).max(y2).min(h as f32 - 1.0) as i32;
    if minx > maxx || miny > maxy {
        return;
    }
    let area = (x1 - x0) * (y2 - y0) - (y1 - y0) * (x2 - x0);
    if area.abs() < 1e-9 {
        return;
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
            // Perspective-correct interpolation via 1/z weights.
            let iz = w0 / v[0].2 + w1 / v[1].2 + w2 / v[2].2;
            let z = 1.0 / iz;
            let idx = y as usize * w as usize + x as usize;
            if z >= zbuf[idx] {
                continue;
            }
            let u = (w0 * uvs[0][0] / v[0].2 + w1 * uvs[1][0] / v[1].2 + w2 * uvs[2][0] / v[2].2) * z;
            let vv = (w0 * uvs[0][1] / v[0].2 + w1 * uvs[1][1] / v[1].2 + w2 * uvs[2][1] / v[2].2) * z;
            let (mut r, mut g, mut b) = (150.0f32, 145.0f32, 135.0f32);
            if let Some((tw, th, rgba)) = texture {
                let tx = ((u.fract() * *tw as f32) as i32).rem_euclid(*tw as i32) as usize;
                let ty = ((vv.fract() * *th as f32) as i32).rem_euclid(*th as i32) as usize;
                let o = (ty * *tw as usize + tx) * 4;
                r = rgba[o] as f32;
                g = rgba[o + 1] as f32;
                b = rgba[o + 2] as f32;
            }
            zbuf[idx] = z;
            let o = idx * 4;
            img[o] = (r * shade).min(255.0) as u8;
            img[o + 1] = (g * shade).min(255.0) as u8;
            img[o + 2] = (b * shade).min(255.0) as u8;
            img[o + 3] = 255;
        }
    }
}
