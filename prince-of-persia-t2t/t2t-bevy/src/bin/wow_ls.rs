//! wow_ls — survey one _wow_ entry of a Jade BIG archive: decompress
//! it, walk its record stream, and classify each record (texture /
//! geometry / other) using the toolkit's detection rules.
//!
//! Usage:
//!   wow_ls <archive.bf> <entry-index | name-substring> [--limit N]
//!            [--save-tex <record#> <out.png>]
//!            [--save-geo <record#> <out.obj>]
//!            [--wireframe <record#> <out.png>]

use std::process::ExitCode;
use t2t::bigfile::BigFile;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::png::write_png_rgba;
use t2t::texture::{decode_texture, parse_texture};
use t2t::wow::walk_wow;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|p| args.get(p + 1).cloned())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: wow_ls <archive.bf> <entry-index|name-substring> [--limit N] [--save-tex R out.png] [--save-geo R out.obj] [--wireframe R out.png]");
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
        bf.active_files()
            .into_iter()
            .find(|e| e.name.contains(&args[2]))
    };
    let Some(entry) = entry else {
        eprintln!("entry not found: {}", args[2]);
        return ExitCode::FAILURE;
    };
    let raw = bf.read_data(entry.index);
    let data = match decompress_entry(raw) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("decompress failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "{} (key 0x{:08x}): {} B stored -> {} B decompressed",
        bf.file_path(entry),
        entry.key,
        raw.len(),
        data.len()
    );
    let records = walk_wow(&data);
    println!("{} records", records.len());

    let limit: usize = arg(&args, "--limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let mut n_tex = 0;
    let mut n_geo = 0;
    let mut n_mrm = 0;
    for (i, r) in records.iter().enumerate() {
        let range = r.data_range();
        let sub = &data[range.clone()];
        let mut what = String::new();
        if let Some(ti) = parse_texture(sub) {
            n_tex += 1;
            what = format!(
                "TEXTURE fmt {} {}x{} ({} mips)",
                ti.format, ti.width, ti.height, ti.mip_count
            );
        } else {
            match parse_geometry(sub) {
                Ok(g) => {
                    n_geo += 1;
                    what = format!(
                        "GEOMETRY v{} {} pts, {} tris, {} elems",
                        g.version,
                        g.nb_points(),
                        g.nb_tris(),
                        g.elements.len()
                    );
                    if let Some((_, bones)) = &g.skin {
                        n_mrm += 1;
                        let wsum: usize = bones.iter().map(|b| b.weights.len()).sum();
                        what += &format!(" + SKIN {} bones, {} weighted verts", bones.len(), wsum);
                    }
                }
                Err(_) => {}
            }
        }
        if i < limit {
            println!(
                "  [{:>4}] key 0x{:08x} {:>9} B  tag={:<6} gro={:<10} {}",
                i,
                r.key,
                r.size,
                if r.tag.is_empty() { "-" } else { &r.tag },
                r.gro_type.map(|g| g.to_string()).unwrap_or_else(|| "-".into()),
                what
            );
        }
    }
    if let Some(p) = args.iter().position(|a| a == "--save-raw") {
        let off: usize = args.get(p + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let len: usize = args.get(p + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        if let Some(out) = args.get(p + 3) {
            let end = (off + len).min(data.len());
            match std::fs::write(out, &data[off..end]) {
                Ok(()) => println!("saved raw [{off}..{end}) -> {out}"),
                Err(e) => eprintln!("write failed: {e}"),
            }
        }
    }
    if args.iter().any(|a| a == "--list-spans") {
        for (i, r) in records.iter().enumerate() {
            println!("[{i}] start {} end {} key 0x{:08x} tag {} gro {:?}", r.offset, r.offset + 12 + r.size as usize, r.key, r.tag, r.gro_type);
        }
    }
    if let Some(p) = args.iter().position(|a| a == "--xref") {
        if let Some(ks) = args.get(p + 1) {
            let want = u32::from_str_radix(ks.trim_start_matches("0x"), 16).unwrap_or(0);
            for (i, r) in records.iter().enumerate() {
                let sub = &data[r.data_range()];
                let hits = sub
                    .windows(4)
                    .filter(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]) == want)
                    .count();
                if hits > 0 {
                    println!(
                        "xref 0x{want:08x}: record [{i}] key 0x{:08x} tag {} gro {:?} size {} hits {}",
                        r.key, r.tag, r.gro_type, sub.len(), hits
                    );
                }
            }
        }
    }
    if args.iter().any(|a| a == "--dump-pose") {
        for r in &records {
            let sub = &data[r.data_range()];
            if let Some(units) = t2t::anim::parse_pose_record(sub) {
                for u in &units {
                    println!(
                        "POSE {:08x} idx {} A {:08x} B {:.5} {:.5} {:.5}",
                        r.key, u.index, u.rotation_raw,
                        u.translation[0], u.translation[1], u.translation[2]
                    );
                }
            }
        }
    }
    if args.iter().any(|a| a == "--dump-bones") {
        for r in &records {
            if r.tag == ".gao" {
                let sub = &data[r.data_range()];
                if let Some(g) = t2t::gao::parse_gao(sub) {
                    if g.name.starts_with("B_") {
                        let lt = g.local_matrix.map(|m| (m[12], m[13], m[14]));
                        let gt = g.global_matrix.map(|m| (m[12], m[13], m[14]));
                        println!(
                            "{:<36} father 0x{:08x} local_t {:?} global_t {:?}",
                            g.name, g.father_key, lt, gt
                        );
                        if let Some(m) = g.local_matrix {
                            println!("    lmat {:?}", m);
                        }
                        println!("    key 0x{:08x}", r.key);
                    }
                }
            }
        }
    }
    if let Some(p) = args.iter().position(|a| a == "--save-record") {
        if let (Some(rs), Some(out)) = (args.get(p + 1), args.get(p + 2)) {
            let rec: usize = rs.parse().unwrap_or(usize::MAX);
            if let Some(r) = records.get(rec) {
                let sub = &data[r.data_range()];
                match std::fs::write(out, sub) {
                    Ok(()) => println!("saved record {rec} data ({} B) -> {out}", sub.len()),
                    Err(e) => eprintln!("write failed: {e}"),
                }
            }
        }
    }
    if let Some(p) = args.iter().position(|a| a == "--dump-record") {
        if let Some(rs) = args.get(p + 1) {
            let rec: usize = rs.parse().unwrap_or(usize::MAX);
            if let Some(r) = records.get(rec) {
                let sub = &data[r.data_range()];
                println!(
                    "record {rec} key 0x{:08x} tag={} gro={:?} data {} B",
                    r.key, r.tag, r.gro_type, sub.len()
                );
                let show = sub.len().min(192);
                for o in (0..show).step_by(4) {
                    let v = u32::from_le_bytes([sub[o], sub[o + 1], sub[o + 2], sub[o + 3]]);
                    let f = f32::from_bits(v);
                    println!("  [{o:>4}] 0x{v:08x}  {v:>12}  f={f:.6}");
                }
            }
        }
    }
    println!("textures: {n_tex}, geometries: {n_geo}, skinned geometries: {n_mrm}");
    if args.iter().any(|a| a == "--census") {
        let mut hist: std::collections::HashMap<String, (usize, usize)> = Default::default();
        for r in &records {
            let k = if !r.tag.is_empty() {
                format!("tag {}", r.tag)
            } else if let Some(g) = r.gro_type {
                format!("gro 0x{g:08x} (hi 0x{:04x} lo 0x{:02x})", g >> 16, g & 0xFF)
            } else {
                "empty".to_string()
            };
            let e = hist.entry(k).or_default();
            e.0 += 1;
            e.1 += r.size as usize;
        }
        let mut rows: Vec<_> = hist.into_iter().collect();
        rows.sort_by_key(|(_, (c, _))| std::cmp::Reverse(*c));
        for (k, (c, bytes)) in rows {
            println!("  {k:<44} {c:>5} records {bytes:>10} B");
        }
    }

    // --save-tex R out.png
    if let Some(p) = args.iter().position(|a| a == "--save-tex") {
        if let (Some(rec_s), Some(out)) = (args.get(p + 1), args.get(p + 2)) {
            let rec: usize = rec_s.parse().unwrap_or(usize::MAX);
            if let Some(r) = records.get(rec) {
                let sub = &data[r.data_range()];
                if let Some(ti) = parse_texture(sub) {
                    let rgba = decode_texture(sub, &ti);
                    if rgba.len() == ti.width as usize * ti.height as usize * 4 {
                        match write_png_rgba(std::path::Path::new(out), ti.width, ti.height, &rgba) {
                            Ok(()) => println!("saved texture {rec} -> {out}"),
                            Err(e) => eprintln!("write failed: {e}"),
                        }
                    } else {
                        eprintln!("decode produced {} bytes, expected {}", rgba.len(), ti.width * ti.height * 4);
                    }
                } else {
                    eprintln!("record {rec} is not a texture");
                }
            }
        }
    }

    // --save-geo R out.obj
    if let Some(p) = args.iter().position(|a| a == "--save-geo") {
        if let (Some(rec_s), Some(out)) = (args.get(p + 1), args.get(p + 2)) {
            let rec: usize = rec_s.parse().unwrap_or(usize::MAX);
            if let Some(r) = records.get(rec) {
                let sub = &data[r.data_range()];
                match parse_geometry(sub) {
                    Ok(g) => {
                        let mut s = String::new();
                        for v in g.vertices.chunks_exact(3) {
                            s += &format!("v {} {} {}\n", v[0], v[1], v[2]);
                        }
                        for f in &g.faces {
                            s += &format!("f {} {} {}\n", f[0] + 1, f[1] + 1, f[2] + 1);
                        }
                        match std::fs::write(out, s) {
                            Ok(()) => println!("saved geometry {rec} -> {out} ({} pts, {} tris)", g.nb_points(), g.nb_tris()),
                            Err(e) => eprintln!("write failed: {e}"),
                        }
                    }
                    Err(e) => eprintln!("record {rec} geometry: {e:?}"),
                }
            }
        }
    }

    // --wireframe R out.png : orthographic XY wireframe, depth-shaded
    if let Some(p) = args.iter().position(|a| a == "--wireframe") {
        if let (Some(rec_s), Some(out)) = (args.get(p + 1), args.get(p + 2)) {
            let rec: usize = rec_s.parse().unwrap_or(usize::MAX);
            if let Some(r) = records.get(rec) {
                let sub = &data[r.data_range()];
                match parse_geometry(sub) {
                    Ok(g) => {
                        if let Err(e) = wireframe(&g, std::path::Path::new(out)) {
                            eprintln!("wireframe failed: {e}");
                        } else {
                            println!("saved wireframe {rec} -> {out}");
                        }
                    }
                    Err(e) => eprintln!("record {rec} geometry: {e:?}"),
                }
            }
        }
    }

    ExitCode::SUCCESS
}

fn wireframe(g: &t2t::geometry::GeoInfo, path: &std::path::Path) -> std::io::Result<()> {
    let (mn, mx) = g.aabb();
    let (w, h) = (900u32, 700u32);
    let mut img = vec![16u8; (w * h * 4) as usize];
    for p in 0..(w * h) as usize {
        img[4 * p + 3] = 255;
    }
    let sx = mx[0] - mn[0];
    let sy = mx[1] - mn[1];
    let sz = (mx[2] - mn[2]).max(1e-6);
    let scale = ((w as f32 - 20.0) / sx.max(1e-6)).min((h as f32 - 20.0) / sy.max(1e-6));
    let proj = |v: &[f32]| -> (i32, i32, f32) {
        let x = 10.0 + (v[0] - mn[0]) * scale;
        let y = h as f32 - 10.0 - (v[1] - mn[1]) * scale;
        (x as i32, y as i32, (v[2] - mn[2]) / sz)
    };
    let verts: Vec<&[f32]> = g.vertices.chunks_exact(3).collect();
    for f in &g.faces {
        let pts = [verts[f[0] as usize], verts[f[1] as usize], verts[f[2] as usize]];
        let pr: Vec<(i32, i32, f32)> = pts.iter().map(|v| proj(v)).collect();
        let depth = (pr[0].2 + pr[1].2 + pr[2].2) / 3.0;
        let bright = (60.0 + 195.0 * depth.clamp(0.0, 1.0)) as u8;
        for e in 0..3 {
            let (x0, y0, _) = pr[e];
            let (x1, y1, _) = pr[(e + 1) % 3];
            draw_line(&mut img, w, h, x0, y0, x1, y1, bright);
        }
    }
    write_png_rgba(path, w, h, &img)
}

fn draw_line(img: &mut [u8], w: u32, h: u32, x0: i32, y0: i32, x1: i32, y1: i32, val: u8) {
    let (mut x, mut y) = (x0, y0);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let o = (y as usize * w as usize + x as usize) * 4;
            img[o] = val;
            img[o + 1] = val;
            img[o + 2] = val;
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}
