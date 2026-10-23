//! t2t_game — the Bevy reimplementation entry point.
//!
//! M1: boots Bevy, indexes the original archive.
//! M3: loads a _wow_ world, builds a Bevy mesh from its largest GEO.
//! M4: assembles the WHOLE world — every GAO's geometry transformed
//! by its global matrix into world space, merged into one mesh with
//! per-corner colours taken from each element's texture average (the
//! texture-binding chain GAO -> material -> texture is resolved in
//! t2t::assemble). The scene is centered on the world AABB and scaled
//! to ~40 units across.
//!
//! Usage: t2t_game [path-to-pop3.bf] [wow-name-substring]
//!   defaults: pop3.bf, "Menu3D"

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::input::mouse::MouseMotion;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::collision::TriangleSoup;
use t2t::controller::{Controller, MoveInput, CLIMB_TIME};
use t2t::jade_stream::decompress_entry;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::texture::decode_texture;
use t2t::wow::walk_wow;

#[derive(Resource)]
struct GameArchive {
    file_count: usize,
    dir_count: usize,
    version: u32,
}

/// De-indexed mesh data ready for Bevy (positions/normals/uvs/colors
/// per face corner, sequential indices).
#[derive(Resource)]
struct WorldMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
    source_name: String,
    tris: usize,
    /// Jade-space center + scale of the world -> Bevy conversion.
    center: [f32; 3],
    scale: f32,
    aabb_min: [f32; 3],
}

/// An animated character: skinned mesh driven by a clip.
/// Corner template: (vertex index, uv) per face corner; positions
/// and flat normals are recomputed every frame.
/// Traversal clip set for the hang state machine.
struct Traversal {
    hang: Clip,
    shimmy: Clip,
    climb: Clip,
}

#[derive(Resource)]
struct CharSpec {
    ch: Character,
    clip: Clip,
    /// Optional airborne clip (state-driven playback).
    clip_air: Option<Clip>,
    clip_air_key: u32,
    traversal: Option<Traversal>,
    map: ChannelMap,
    corners: Vec<(u16, [f32; 2])>,
    /// Jade-space spawn (feet), her real level spot.
    spawn: [f32; 3],
    soup: TriangleSoup,
    center: [f32; 3],
    scale: f32,
    /// Her height in Bevy units (for camera framing).
    bevy_height: f32,
    name: &'static str,
    clip_key: u32,
}

#[derive(Component)]
struct AnimatedChar {
    mesh: Handle<Mesh>,
    time: f32,
    /// Airborne clock — runs only while not grounded, so the
    /// air clip restarts on every jump/fall.
    air_time: f32,
    /// Hang clock — runs while hanging (hang/shimmy clips).
    hang_time: f32,
    ctl: Controller,
    orbit_yaw: f32,
    orbit_pitch: f32,
}

/// Marks the follow camera.
#[derive(Component)]
struct FollowCam;

/// Jade (Z-up, ~100 units per metre) -> Bevy conversion shared
/// with the world mesh.
fn jade_to_bevy(p: [f32; 3], center: [f32; 3], s: f32) -> [f32; 3] {
    [
        (p[0] - center[0]) * s,
        (p[2] - center[2]) * s,
        (p[1] - center[1]) * s,
    ]
}

fn char_corners(ch: &Character, clip: &Clip, map: &ChannelMap, frame: u32, spec_p: [f32; 3], yaw: f32, center: [f32; 3], s: f32) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let verts = posed_verts(ch, clip, map, frame);
    // Skinned verts are in metres; the level is ~100x. Rotate
    // around the feet by yaw (model faces +Y at rest, hence the
    // -90 deg offset against the controller's atan2 yaw).
    let rot = yaw - std::f32::consts::FRAC_PI_2;
    let (cs, sn) = (rot.cos(), rot.sin());
    let to_bevy = |vi: u16| -> [f32; 3] {
        let o = vi as usize * 3;
        let (lx, ly) = (verts[o] * 100.0, verts[o + 1] * 100.0);
        jade_to_bevy(
            [
                lx * cs - ly * sn + spec_p[0],
                lx * sn + ly * cs + spec_p[1],
                verts[o + 2] * 100.0 + spec_p[2],
            ],
            center,
            s,
        )
    };
    let mut positions = Vec::with_capacity(ch.faces.len() * 3);
    let mut normals = Vec::with_capacity(ch.faces.len() * 3);
    for f in &ch.faces {
        let (a, b, c) = (to_bevy(f[0]), to_bevy(f[1]), to_bevy(f[2]));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
        n = [n[0] / l, n[1] / l, n[2] / l];
        positions.extend([a, b, c]);
        normals.extend([n, n, n]);
    }
    (positions, normals)
}

fn prepare_world(bf_path: &str, wow_name: &str) -> Option<WorldMesh> {
    let bf = BigFile::open(bf_path).ok()?;
    let entry = bf.active_files().into_iter().find(|e| e.name.contains(wow_name))?;
    let data = decompress_entry(bf.read_data(entry.index)).ok()?;
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    if world.placed.is_empty() {
        return None;
    }
    // Texture average colours per key (for vertex colours).
    let mut avg: std::collections::HashMap<u32, [f32; 4]> = Default::default();
    for (key, (ti, range)) in &world.textures {
        let rgba = decode_texture(&data[range.clone()], ti);
        if !rgba.is_empty() {
            let mut acc = [0f32; 3];
            let n = (rgba.len() / 4).max(1);
            for px in rgba.chunks_exact(4) {
                acc[0] += px[0] as f32;
                acc[1] += px[1] as f32;
                acc[2] += px[2] as f32;
            }
            avg.insert(
                *key,
                [acc[0] / n as f32 / 255.0, acc[1] / n as f32 / 255.0, acc[2] / n as f32 / 255.0, 1.0],
            );
        }
    }

    let (mn, mx) = world.aabb();
    let center = [
        (mn[0] + mx[0]) / 2.0,
        (mn[1] + mx[1]) / 2.0,
        (mn[2] + mx[2]) / 2.0,
    ];
    let extent = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]).max(1e-6);
    let s = 40.0 / extent;

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    let mut tris = 0usize;
    for placed in &world.placed {
        let geo = &placed.geo;
        let wv = &placed.world_vertices;
        let vpos = |i: u16| -> [f32; 3] {
            let o = i as usize * 3;
            [
                (wv[o] - center[0]) * s,
                (wv[o + 2] - center[2]) * s, // Jade Z-up -> Bevy Y-up
                (wv[o + 1] - center[1]) * s,
            ]
        };
        for f in &geo.faces {
            tris += 1;
            let (a, b, c) = (vpos(f[0]), vpos(f[1]), vpos(f[2]));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let mut n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            n = [n[0] / l, n[1] / l, n[2] / l];
            let col = placed
                .element_textures
                .get(f[6] as usize)
                .copied()
                .flatten()
                .and_then(|k| avg.get(&k).copied())
                .unwrap_or([0.6, 0.58, 0.54, 1.0]);
            for k in 0..3 {
                positions.push(vpos(f[k]));
                normals.push(n);
                let uvi = f[3 + k] as usize * 2;
                if uvi + 1 < geo.uvs.len() {
                    uvs.push([geo.uvs[uvi], geo.uvs[uvi + 1]]);
                } else {
                    uvs.push([0.0, 0.0]);
                }
                colors.push(col);
                indices.push(indices.len() as u32);
            }
        }
    }
    Some(WorldMesh {
        tris,
        positions,
        normals,
        uvs,
        colors,
        indices,
        source_name: bf.file_path(entry),
        center,
        scale: s,
        aabb_min: mn,
    })
}

/// Character presets per sector: (geo key, bone prefix, clip
/// key, header key if any, spawn anchor GAO, height in Jade
/// units, display name).
struct Preset {
    geo: u32,
    prefix: &'static str,
    clip: u32,
    header: Option<u32>,
    anchor: &'static str,
    height: f32,
    name: &'static str,
    /// Airborne clip: (sector substring, clip key), loaded
    /// cross-sector — rigs and clip banks live in different
    /// wows (e.g. the Prince's bank is AMIR_wow).
    air_clip: Option<(&'static str, u32)>,
    /// Spawn height above the floor (a tall spawn makes the
    /// spawn fall read as the dive it is).
    spawn_lift: f32,
    /// Sector holding the rig + body GEO ("" = the world
    /// sector itself).
    rig_sector: &'static str,
    /// Traversal clips (hang, shimmy, climb) + their sector.
    traversal: Option<(&'static str, u32, u32, u32)>,
}

fn preset_for(wow_name: &str) -> Option<Preset> {
    // T2T_CHAR=prince|kaileena forces a character into any
    // world sector (the rig loads from its home sector).
    let forced = std::env::var("T2T_CHAR").unwrap_or_default();
    if forced == "prince" {
        return Some(Preset {
            geo: 0x510022d3,
            prefix: "B_Pr",
            clip: 0x42004cd3,
            header: None,
            anchor: "M_PrinceBare",
            height: 190.0,
            name: "Prince",
            air_clip: Some(("AMIR_wow", 0x33000ee7)),
            spawn_lift: 250.0,
            rig_sector: "PrinceFinal_Shape",
            traversal: Some(("AMIR_wow", 0x33000811, 0x33000f52, 0x970000d3)),
        });
    }
    if wow_name.contains("Quai") {
        // Kaileena, clip via her header remap table.
        Some(Preset {
            geo: 0x420035b0,
            prefix: "B_Kaileena",
            clip: 0x420035a0,
            header: Some(0x42000419),
            anchor: "FAKE_Kaileena",
            height: 184.0,
            name: "Kaileena",
            air_clip: None,
            spawn_lift: 2.0,
            rig_sector: "",
            traversal: None,
        })
    } else if wow_name.contains("PrinceFinal") {
        // The Prince (bare variant), standard-bip identity map.
        Some(Preset {
            geo: 0x510022d3,
            prefix: "B_Pr",
            clip: 0x42004cd3,
            header: None,
            anchor: "M_PrinceBare",
            height: 190.0,
            name: "Prince",
            // His dive/glide clip from the AMIR_wow bank.
            air_clip: Some(("AMIR_wow", 0x33000ee7)),
            spawn_lift: 250.0,
            rig_sector: "PrinceFinal_Shape",
            // Verified traversal set from the AMIR bank:
            // hang idle, wall shimmy, ledge pull-up.
            traversal: Some(("AMIR_wow", 0x33000811, 0x33000f52, 0x970000d3)),
        })
    } else {
        None
    }
}

fn prepare_character(bf_path: &str, wow_name: &str, world: &WorldMesh) -> Option<CharSpec> {
    let preset = preset_for(wow_name)?;
    let bf = BigFile::open(bf_path).ok()?;
    let load = |substr: &str| -> Option<Vec<u8>> {
        let e = bf.active_files().into_iter().find(|e| e.name.contains(substr))?;
        decompress_entry(bf.read_data(e.index)).ok()
    };
    let load_clip = |sector: &str, key: u32| -> Option<Clip> {
        let d = load(sector)?;
        let recs = walk_wow(&d);
        let rec = recs.iter().find(|r| r.key == key)?;
        Some(Clip::load(&d[rec.data_range()]))
    };
    // World data (collision + spawn) vs rig data (character
    // + home clips) — different sectors for guest characters.
    let data = load(wow_name)?;
    let rig_data = if preset.rig_sector.is_empty() {
        data.clone()
    } else {
        load(preset.rig_sector)?
    };
    let ch = Character::load(&rig_data, preset.geo, preset.prefix)?;
    let recs = walk_wow(&rig_data);
    let clip_rec = recs.iter().find(|r| r.key == preset.clip)?;
    let clip = Clip::load(&rig_data[clip_rec.data_range()]);
    let map = match preset.header {
        Some(hk) => {
            let hdr = recs.iter().find(|r| r.key == hk)?;
            ChannelMap::from_header(&rig_data[hdr.data_range()])
        }
        None => ChannelMap::Identity,
    };
    // Cross-sector airborne clip (banks live in other wows).
    let (clip_air, clip_air_key) = match preset.air_clip {
        Some((sector, key)) => (Some(load_clip(sector, key)?), key),
        None => (None, 0),
    };
    // Traversal clip set (hang / shimmy / climb).
    let traversal = match preset.traversal {
        Some((sector, hang, shimmy, climb)) => Some(Traversal {
            hang: load_clip(sector, hang)?,
            shimmy: load_clip(sector, shimmy)?,
            climb: load_clip(sector, climb)?,
        }),
        None => None,
    };
    let mut corners = Vec::with_capacity(ch.faces.len() * 3);
    for f in &ch.faces {
        for k in 0..3 {
            let uvi = f[3 + k] as usize * 2;
            let uv = if uvi + 1 < ch.uvs.len() {
                [ch.uvs[uvi], ch.uvs[uvi + 1]]
            } else {
                [0.0, 0.0]
            };
            corners.push((f[k], uv));
        }
    }
    // The character's real level spot: the anchor GAO's AABB
    // in the assembled WORLD sector; feet snapped to the
    // floor. Anchor fallback chain for guest characters.
    let records2 = walk_wow(&data);
    let world2 = assemble(&data, &records2);
    let soup = TriangleSoup::from_world(&world2);
    let fake = world2
        .placed
        .iter()
        .find(|p| p.gao_name.contains(preset.anchor))
        .or_else(|| world2.placed.iter().find(|p| p.gao_name.contains("FAKE_Kaileena")))
        .or_else(|| world2.placed.iter().find(|p| p.gao_name.contains("FAKE")));
    let (sx, sy, gz) = match fake {
        Some(fake) => {
            let mut mn = [f32::INFINITY; 3];
            let mut mx = [f32::NEG_INFINITY; 3];
            for v in fake.world_vertices.chunks_exact(3) {
                for i in 0..3 {
                    mn[i] = mn[i].min(v[i]);
                    mx[i] = mx[i].max(v[i]);
                }
            }
            let (sx, sy) = ((mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0);
            let gz = soup.ground_height(sx, sy, mx[2] + 400.0, 30.0).unwrap_or(mn[2]);
            (sx, sy, gz)
        }
        None => {
            let (mn, mx) = world2.aabb();
            ((mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0, mx[2])
        }
    };
    // Optional spawn offset for staged demos (Jade units).
    let (ox, oy, oz) = match std::env::var("T2T_SPAWN_OFF") {
        Ok(v) => {
            let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            if p.len() == 3 { (p[0], p[1], p[2]) } else { (0.0, 0.0, 0.0) }
        }
        Err(_) => (0.0, 0.0, 0.0),
    };
    let spawn = [sx + ox, sy + oy, gz + preset.spawn_lift + oz];
    println!("[t2t] character spawn (Jade): {spawn:?}");
    Some(CharSpec {
        ch,
        clip,
        clip_air,
        clip_air_key,
        traversal,
        map,
        corners,
        spawn,
        soup,
        center: world.center,
        scale: world.scale,
        bevy_height: preset.height * world.scale,
        name: preset.name,
        clip_key: preset.clip,
    })
}

#[allow(clippy::too_many_arguments)]
fn animate_char(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    spec: Res<CharSpec>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<&mut AnimatedChar>,
    mut cam: Query<&mut Transform, With<FollowCam>>,
) {
    let dt = time.delta_secs().min(0.05);
    for mut ac in &mut q {
        // Orbit: mouse drag (LMB held) or arrow keys.
        if mouse_buttons.pressed(MouseButton::Left) {
            for ev in mouse_motion.read() {
                ac.orbit_yaw -= ev.delta.x * 0.005;
                ac.orbit_pitch = (ac.orbit_pitch + ev.delta.y * 0.005).clamp(-0.2, 1.0);
            }
        } else {
            mouse_motion.clear();
        }
        if keys.pressed(KeyCode::ArrowLeft) {
            ac.orbit_yaw += dt * 1.8;
        }
        if keys.pressed(KeyCode::ArrowRight) {
            ac.orbit_yaw -= dt * 1.8;
        }

        // Movement input in camera-relative Jade XY.
        let (sin, cos) = ac.orbit_yaw.sin_cos();
        let fwd = [cos, sin];
        let right = [-sin, cos];
        let mut wish = [0f32; 2];
        if keys.pressed(KeyCode::KeyW) {
            wish = [wish[0] + fwd[0], wish[1] + fwd[1]];
        }
        if keys.pressed(KeyCode::KeyS) {
            wish = [wish[0] - fwd[0], wish[1] - fwd[1]];
        }
        if keys.pressed(KeyCode::KeyD) {
            wish = [wish[0] + right[0], wish[1] + right[1]];
        }
        if keys.pressed(KeyCode::KeyA) {
            wish = [wish[0] - right[0], wish[1] - right[1]];
        }
        let wl = (wish[0] * wish[0] + wish[1] * wish[1]).sqrt();
        let speed = if keys.pressed(KeyCode::ShiftLeft) { 340.0 } else { 170.0 };
        let input = MoveInput {
            vel_xy: if wl > 0.01 {
                [wish[0] / wl * speed, wish[1] / wl * speed]
            } else {
                [0.0, 0.0]
            },
            jump: keys.just_pressed(KeyCode::Space),
            drop: keys.just_pressed(KeyCode::KeyS),
        };
        // Fixed-substep physics for stability.
        let steps = 2;
        for _ in 0..steps {
            ac.ctl.step(&spec.soup, &input, dt / steps as f32);
        }

        // Animation time + mesh rebuild. State-driven clip
        // choice: hanging -> traversal set, airborne -> the
        // air clip (own clock), grounded -> the main clip.
        ac.time += dt;
        let mut fixed_frame: Option<u32> = None;
        let (active_clip, active_time) =
            if let (Some(trav), Some(h)) = (&spec.traversal, ac.ctl.hanging) {
                ac.hang_time += dt;
                if h.climbing > 0.0 {
                    let f = (h.climbing / CLIMB_TIME * trav.climb.max_frame as f32) as u32;
                    fixed_frame = Some(f.min(trav.climb.max_frame));
                    (&trav.climb, 0.0)
                } else if wl > 0.01 {
                    (&trav.shimmy, ac.hang_time)
                } else {
                    (&trav.hang, ac.hang_time)
                }
            } else {
                ac.hang_time = 0.0;
                match (&spec.clip_air, ac.ctl.grounded) {
                    (Some(air), false) => {
                        ac.air_time += dt;
                        (air, ac.air_time)
                    }
                    _ => {
                        ac.air_time = 0.0;
                        (&spec.clip, ac.time)
                    }
                }
            };
        let frame = fixed_frame
            .unwrap_or_else(|| ((active_time * 30.0) as u32) % (active_clip.max_frame + 1));
        let (positions, normals) = char_corners(
            &spec.ch,
            active_clip,
            &spec.map,
            frame,
            ac.ctl.pos,
            ac.ctl.yaw,
            spec.center,
            spec.scale,
        );
        if let Some(mesh) = meshes.get_mut(&ac.mesh) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        }

        // Follow camera: behind + above her, looking at her.
        // Distance is relative to her height — the world scale
        // varies per level, absolute units do not.
        let char_bevy = jade_to_bevy(ac.ctl.pos, spec.center, spec.scale);
        let h = spec.bevy_height;
        let dist = h * 2.6;
        let height = h * (0.9 + ac.orbit_pitch * 1.2);
        let cam_pos = Vec3::new(
            char_bevy[0] - cos * dist,
            char_bevy[1] + height,
            char_bevy[2] - sin * dist,
        );
        for mut t in &mut cam {
            *t = Transform::from_translation(cam_pos)
                .looking_at(Vec3::new(char_bevy[0], char_bevy[1] + h * 0.55, char_bevy[2]), Vec3::Y);
        }
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    archive: Option<Res<GameArchive>>,
    world: Option<Res<WorldMesh>>,
    char_spec: Option<Res<CharSpec>>,
) {
    if let Some(a) = archive {
        println!(
            "[t2t] engine ready — BIG v{}, {} files, {} dirs indexed",
            a.version, a.file_count, a.dir_count
        );
    }
    if let Some(w) = world {
        println!(
            "[t2t] world mesh: {} ({} tris, {} corners)",
            w.source_name,
            w.tris,
            w.positions.len()
        );
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, w.positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, w.normals.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, w.uvs.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, w.colors.clone());
        mesh.insert_indices(Indices::U32(w.indices.clone()));
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                ..default()
            })),
            Transform::default(),
        ));
    }
    if let Some(spec) = char_spec {
        let (positions, normals) = char_corners(
            &spec.ch,
            &spec.clip,
            &spec.map,
            0,
            spec.spawn,
            0.0,
            spec.center,
            spec.scale,
        );
        let n = positions.len();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_UV_0,
            spec.corners.iter().map(|c| c.1).collect::<Vec<_>>(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_COLOR,
            vec![[0.85f32, 0.79, 0.7, 1.0]; n],
        );
        mesh.insert_indices(Indices::U32((0..n as u32).collect()));
        let handle = meshes.add(mesh);
        println!(
            "[t2t] character: {}, clip {:#010x} ({} frames, {} corners)",
            spec.name,
            spec.clip_key,
            spec.clip.max_frame + 1,
            n
        );
        if let Some(air) = &spec.clip_air {
            println!(
                "[t2t] air clip: {:#010x} ({} frames, cross-sector)",
                spec.clip_air_key,
                air.max_frame + 1
            );
        }
        let mut ctl0 = Controller::new(spec.spawn);
        if let Ok(v) = std::env::var("T2T_SPAWN_YAW") {
            if let Ok(y) = v.parse::<f32>() {
                ctl0.yaw = y;
            }
        }
        commands.spawn((
            Mesh3d(handle.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                ..default()
            })),
            Transform::default(),
            AnimatedChar {
                mesh: handle,
                time: 0.0,
                air_time: 0.0,
                hang_time: 0.0,
                ctl: ctl0,
                orbit_yaw: 0.0,
                orbit_pitch: 0.25,
            },
        ));
    }
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(26.0, 16.0, 34.0).looking_at(Vec3::ZERO, Vec3::Y),
        FollowCam,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12000.0,
            ..default()
        },
        Transform::from_xyz(8.0, 14.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.65, 0.68, 0.75),
        brightness: 400.0,
        affects_lightmapped_meshes: false,
    });
}

fn main() {
    let bf_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "pop3.bf".to_string());
    let wow_name = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "Menu3D".to_string());

    let mut app = App::new();
    app.add_plugins(DefaultPlugins);

    match BigFile::open(&bf_path) {
        Ok(bf) => {
            println!(
                "[t2t] archive loaded: BIG v{}, {} active files, {} dirs",
                bf.version,
                bf.active_files().len(),
                bf.active_dir_count()
            );
            app.insert_resource(GameArchive {
                file_count: bf.active_files().len(),
                dir_count: bf.active_dir_count(),
                version: bf.version,
            });
        }
        Err(e) => {
            eprintln!("[t2t] could not load archive {bf_path}: {e}");
        }
    }
    let mut have_char = false;
    if let Some(world) = prepare_world(&bf_path, &wow_name) {
        if let Some(spec) = prepare_character(&bf_path, &wow_name, &world) {
            app.insert_resource(spec);
            have_char = true;
        }
        app.insert_resource(world);
    } else {
        eprintln!("[t2t] no parseable geometry found in wow matching '{wow_name}'");
    }

    app.add_systems(Startup, setup);
    if have_char {
        app.add_systems(Update, animate_char);
    }
    app.run();
}
