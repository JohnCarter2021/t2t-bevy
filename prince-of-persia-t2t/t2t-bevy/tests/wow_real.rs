//! M3 verification against the real Menu3D world in pop3.bf:
//! record-stream walk, texture/geometry census, and exact decodes.
//! Numbers measured 2026-10-22 and cross-checked visually (the
//! record-772 flame atlas and record-398 wireframe previews).

use t2t::bigfile::BigFile;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::texture::{decode_texture, parse_texture};
use t2t::wow::walk_wow;

#[test]
fn menu3d_world_parses() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf");
    if !p.exists() {
        eprintln!("pop3.bf not present — skipping");
        return;
    }
    let bf = BigFile::open(&p).expect("archive");
    let entry = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("Menu3D"))
        .expect("Menu3D entry");
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    assert_eq!(data.len(), 8_949_269);

    let records = walk_wow(&data);
    assert_eq!(records.len(), 775);

    let mut n_tex = 0;
    let mut n_geo = 0;
    for r in &records {
        let sub = &data[r.data_range()];
        if parse_texture(sub).is_some() {
            n_tex += 1;
        } else if parse_geometry(sub).is_ok() {
            n_geo += 1;
        }
    }
    assert_eq!(n_tex, 128);
    assert_eq!(n_geo, 76);

    // Record 772: the DXT5 flame atlas — decodes to a full 128x128 RGBA.
    let sub = &data[records[772].data_range()];
    let ti = parse_texture(sub).expect("texture 772");
    assert_eq!((ti.format, ti.width, ti.height), (7, 128, 128));
    let rgba = decode_texture(sub, &ti);
    assert_eq!(rgba.len(), 128 * 128 * 4);

    // Record 398: largest geometry — 4,114 points, 2,835 triangles.
    let sub = &data[records[398].data_range()];
    let g = parse_geometry(sub).expect("geometry 398");
    assert_eq!(g.nb_points(), 4114);
    assert_eq!(g.nb_tris(), 2835);

    // M4: full assembly — GAOs, transforms, material/texture binding.
    // Numbers measured 2026-10-22; the assembled render (skybox ring
    // around the Tower of Babylon island) was visually confirmed as
    // the game's main-menu scene.
    let world = t2t::assemble::assemble(&data, &records);
    assert_eq!(world.stats.gaos, 353);
    assert_eq!(world.stats.gaos_with_visual, 290);
    assert_eq!(world.stats.geos_placed, 102);
    assert_eq!(world.total_tris(), 84_887);
    assert_eq!(world.stats.elements, 798);
    assert_eq!(world.stats.elements_with_texture_key, 798);
    assert_eq!(world.stats.texture_keys_missing, 0);
}
