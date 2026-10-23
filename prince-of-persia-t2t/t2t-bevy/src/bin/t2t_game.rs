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
use t2t::combat::{Fighter, FighterState, SandTank, StrikeProfile};
use t2t::controller::{Checkpoint, Controller, MoveInput, CLIMB_TIME};
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
/// Pole clip set for the column-grip state.
struct PoleClips {
    grip: Clip,
    climb: Clip,
    spin: Clip,
}

/// A combat opponent: rig, clips, and spawn, loaded
/// cross-sector like everything else.
struct EnemySpec {
    ch: Character,
    /// Channel map for this enemy's clips (identity, or
    /// name-based for cross-rig pool clips).
    map: ChannelMap,
    corners: Vec<(u16, [f32; 2])>,
    move_cycle: Clip,
    pounce: Clip,
    hit: Clip,
    death: Clip,
    spawn: [f32; 3],
    name: &'static str,
    height: f32,
}

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
    pole_clips: Option<PoleClips>,
    clip_attack: Option<Clip>,
    clip_hit: Option<Clip>,
    enemies: Vec<EnemySpec>,
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
    /// Falling below this Jade z = out of the world.
    kill_z: f32,
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
    /// Pole clock — runs while gripping a column.
    pole_time: f32,
    ctl: Controller,
    orbit_yaw: f32,
    orbit_pitch: f32,
    fighter: Fighter,
    /// Sand resource: kills feed it, the R power spends it.
    sand: SandTank,
    /// Seconds of slow-motion remaining (0 = off).
    slowmo_t: f32,
    /// Real seconds since spawn (staging triggers).
    elapsed: f32,
    /// The T2T_SLOWMO_AT staging trigger fires once.
    slowmo_staged: bool,
    /// Last safe grounded spot — the respawn point.
    checkpoint: Checkpoint,
}

/// The combat opponent, driven by a simple AI in the same
/// system as the player (single-writer for both fighters).
#[derive(Component)]
struct EnemyChar {
    /// Index into CharSpec.enemies.
    idx: usize,
    mesh: Handle<Mesh>,
    ctl: Controller,
    fighter: Fighter,
    /// Free-running clock for looping clips.
    time: f32,
    /// Seconds since death (death clip progress).
    dead_time: f32,
    home: [f32; 3],
    /// This death has already paid its sand to the Prince.
    sand_paid: bool,
}

/// Marks the combat HUD text.
#[derive(Component)]
struct HudText;

/// Sand economy: capacity, the kill bounty, and the
/// slow-motion power (R key): cost, duration, and the
/// per-actor time scales while it runs.
const SAND_MAX: u32 = 6;
const SAND_PER_KILL: u32 = 2;
const SLOWMO_COST: u32 = 1;
const SLOWMO_SECS: f32 = 4.0;
const SLOWMO_ENEMY_SCALE: f32 = 0.3;
const SLOWMO_PLAYER_SCALE: f32 = 0.9;

/// A pre-activated checkpoint (staging: T2T_CHECKPOINT=
/// x,y,z — stands in for a level-placed point the player
/// already reached).
fn seed_checkpoint() -> Checkpoint {
    let mut c = Checkpoint::new();
    if let Ok(v) = std::env::var("T2T_CHECKPOINT") {
        let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if p.len() == 3 {
            c.seed([p[0], p[1], p[2]], 0.0);
            println!("[t2t] checkpoint seeded at {:?}", c.point);
        }
    }
    c
}

/// Starting sand (staging override: T2T_SAND=<n>).
fn start_sand() -> u32 {
    std::env::var("T2T_SAND")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn player_fighter() -> Fighter {
    Fighter::new(
        5,
        StrikeProfile {
            duration: 0.30,
            active: (0.08, 0.22),
            range: 250.0,
            half_angle: 1.15,
            damage: 1,
        },
    )
}

fn enemy_fighter() -> Fighter {
    Fighter::new(
        3,
        StrikeProfile {
            duration: 0.63,
            active: (0.20, 0.45),
            range: 265.0,
            half_angle: 1.2,
            damage: 1,
        },
    )
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
    /// Combat setup: the player's attack/hit clips.
    combat: Option<CombatSetup>,
    /// Pole clips from Dynamic_Actions_Column:
    /// (grip, climb, spin).
    pole: Option<(u32, u32, u32)>,
    /// Traversal clips (hang, shimmy, climb) + their sector.
    traversal: Option<(&'static str, u32, u32, u32)>,
}

/// One combat opponent's definition: its own bank sector
/// holds both the rig and the clips (all visually
/// verified on that rig).
#[derive(Clone, Copy)]
struct EnemySetup {
    /// Sector holding the CLIPS.
    sector: &'static str,
    /// Sector holding the RIG, when different from the
    /// clip sector (the Babylonian Soldier's rig lives in
    /// 100_Quai; his clips are the shared pool).
    rig_sector: Option<&'static str>,
    geo: u32,
    prefix: &'static str,
    /// Reference rig (sector, geo, prefix) whose bone
    /// order indexes the clips, when the clips were not
    /// authored on this rig: build a name-based channel
    /// map from it. None = identity.
    map_from: Option<(&'static str, u32, &'static str)>,
    /// (move cycle, pounce, hit-react, death).
    clips: (u32, u32, u32, u32),
    name: &'static str,
    height: f32,
}

/// Combat wiring for a preset: the player's strike clips
/// and the enemy roster.
#[derive(Clone, Copy)]
struct CombatSetup {
    /// (sector, key): the player's attack swing.
    attack: (&'static str, u32),
    /// (sector, key): the player's hit reaction.
    hit: (&'static str, u32),
    enemies: &'static [EnemySetup],
}

/// The Prince's roster. The Sand Garde fights from his
/// own rig and bank (ACT_Enemy_SandGarde); the Human
/// Guard from his (ACT_Enemy_HumanGarde, rig also in
/// 100_Quai). Both play the shared enemy pool coherently
/// on their own rigs — lunging cycle 6c0019e5, pounce
/// 6c000506, knockdown-rise a20008ca, death a2000ac3 —
/// verified pose-for-pose against the Sand Garde survey.
/// Every other resolved enemy (the Babylonian Soldier
/// first, then the full resolve_banks census) plays the
/// pool through the NAME-BASED channel map from the Sand
/// Garde's bone order: their rigs order the same Bip
/// bones differently, and the map re-pairs channels by
/// bone name. Pair counts (of SG's 46 channels) and the
/// visual verdicts live in docs/FORMATS.md. NOT
/// rostered: Chameleon, DarkReptus, Hunter (creature
/// rigs), plus Illusion / Rowan / Pigeon (no fighter
/// rig in bank).
/// Mahasti is the exception that proves the rule: the
/// pool slabs her loincloth, but her OWN bank carries a
/// dedicated 0x6603xxxx moveset that plays on her rig by
/// IDENTITY (cycle a66a stepping turn, pounce a66d spin
/// flourish, hit a649 coil recoil, death a3f6 forward
/// collapse — strips in out/mahasti_*_identity.png).
/// T2T_ENEMIES selects who spawns: comma-separated name
/// fragments, or "all"; unset = the original trio. The
/// Prince strikes with the SpeedKill spin slash
/// 4e001719 and reels with the AMIR knockback 970005d0.
const PRINCE_COMBAT: CombatSetup = CombatSetup {
    attack: ("Dynamic_Actions_SpeedKill_prince", 0x4e001719),
    hit: ("AMIR_wow", 0x970005d0),
    enemies: &[
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: None,
            map_from: None,
            geo: 0x51001f50,
            prefix: "B_SG",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Sand Garde",
            height: 175.0,
        },
        EnemySetup {
            sector: "ACT_Enemy_HumanGarde",
            rig_sector: None,
            map_from: None,
            geo: 0x5100019f,
            prefix: "B_HG",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Human Guard",
            height: 180.0,
        },
        // The Babylonian Soldier: his rig (in 100_Quai)
        // orders the same Bip bones differently (arms
        // before legs, no skirt bones), so the shared
        // pool needs the name-based channel map from the
        // Sand Garde's bone order — with it, the pool
        // plays on him pose-for-pose (soldier_map survey).
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("100_Quai"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x42003196,
            prefix: "B_BabylonSoldier",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Babylonian Soldier",
            height: 192.0,
        },
        // Resolved by the resolve_banks census (name map
        // from the Sand Garde, pool clips, strips verified
        // pose-for-pose): SandGateGarde 42/46 pairs, Thrall
        // 39/46, DarkWarrior 36/46, both archers 35/46,
        // both Twins 34/46, Enchantress 31/46.
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("ACT_Enemy_SandGateGarde"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x51001f9d,
            prefix: "B_SGG",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Sand Gate Garde",
            height: 190.0,
        },
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("ACT_Enemy_Thrall"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x51003f51,
            prefix: "B_Thrall",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Thrall",
            height: 200.0,
        },
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("ACT_Enemy_DarkWarrior"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x5100417f,
            prefix: "B_DW",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Dark Warrior",
            height: 190.0,
        },
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("ACT_Enemy_SandArcher"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x51002023,
            prefix: "B_SA",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Sand Archer",
            height: 180.0,
        },
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("ACT_Enemy_HumanArcher"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x51001bdc,
            prefix: "B_HA",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Human Archer",
            height: 180.0,
        },
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("ACT_Enemy_Twins"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x42002e1c,
            prefix: "B_Twins",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Twin Warrior I",
            height: 185.0,
        },
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("ACT_Enemy_Twins"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x42002e93,
            prefix: "B_Twins",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Twin Warrior II",
            height: 185.0,
        },
        EnemySetup {
            sector: "ACT_Enemy_SandGarde",
            rig_sector: Some("ACT_Enemy_Enchantress"),
            map_from: Some(("ACT_Enemy_SandGarde", 0x51001f50, "B_SG")),
            geo: 0x42002ac9,
            prefix: "B_Enchantress",
            clips: (0x6c0019e5, 0x6c000506, 0xa20008ca, 0xa2000ac3),
            name: "Enchantress",
            height: 170.0,
        },
        // Mahasti: her own bank, her own moveset, identity
        // binding (see the note above PRINCE_COMBAT).
        EnemySetup {
            sector: "ACT_Enemy_Mahasti",
            rig_sector: None,
            map_from: None,
            geo: 0x42002c7d,
            prefix: "B_KA",
            clips: (0x6603a66a, 0x6603a66d, 0x6603a649, 0x6603a3f6),
            name: "Mahasti",
            height: 185.0,
        },
    ],
};

/// T2T_ENEMIES roster selection: unset = the original
/// trio (the roster's first three); "all" = everyone;
/// otherwise comma-separated name fragments, matched
/// case-insensitively (an empty match falls back to all
/// with a warning rather than an empty fight).
fn select_enemies(roster: &'static [EnemySetup]) -> Vec<EnemySetup> {
    match std::env::var("T2T_ENEMIES") {
        Err(_) => roster.iter().take(3).copied().collect(),
        Ok(v) if v.trim().eq_ignore_ascii_case("all") => roster.to_vec(),
        Ok(v) => {
            // Match ignoring case AND spaces, so "sandgate"
            // finds "Sand Gate Garde".
            let squash = |s: &str| s.to_lowercase().replace(' ', "");
            let toks: Vec<String> = v
                .split(',')
                .map(|t| squash(t.trim()))
                .filter(|t| !t.is_empty())
                .collect();
            let picked: Vec<EnemySetup> = roster
                .iter()
                .filter(|e| toks.iter().any(|t| squash(e.name).contains(t)))
                .copied()
                .collect();
            if picked.is_empty() {
                println!("[t2t] T2T_ENEMIES={v:?} matched nobody; using the full roster");
                roster.to_vec()
            } else {
                picked
            }
        }
    }
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
            combat: Some(PRINCE_COMBAT),
            pole: Some((0x4e00178a, 0x4e001784, 0x4e001796)),
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
            combat: None,
            pole: None,
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
            combat: Some(PRINCE_COMBAT),
            pole: Some((0x4e00178a, 0x4e001784, 0x4e001796)),
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
    // Combat: player strike clips + the enemy, loaded from
    // their own sectors.
    let (clip_attack, clip_hit, enemy_setups) = match preset.combat {
        Some(c) => (
            Some(load_clip(c.attack.0, c.attack.1)?),
            Some(load_clip(c.hit.0, c.hit.1)?),
            select_enemies(c.enemies),
        ),
        None => (None, None, Vec::new()),
    };
    let mut enemies: Vec<EnemySpec> = Vec::new();
    for e in &enemy_setups {
        let edata = load(e.sector)?;
        let rig_data = match e.rig_sector {
            Some(rs) => load(rs)?,
            None => edata.clone(),
        };
        let ech = Character::load(&rig_data, e.geo, e.prefix)?;
        let emap = match e.map_from {
            Some((msec, mgeo, mpre)) => {
                let mdata = load(msec)?;
                let mch = Character::load(&mdata, mgeo, mpre)?;
                ChannelMap::from_bone_names(&mch, mpre, &ech, e.prefix)
            }
            None => ChannelMap::Identity,
        };
        let erecs = walk_wow(&edata);
        let eget = |key: u32| -> Option<Clip> {
            let r = erecs.iter().find(|r| r.key == key)?;
            Some(Clip::load(&edata[r.data_range()]))
        };
        let mut ecorners = Vec::with_capacity(ech.faces.len() * 3);
        for f in &ech.faces {
            for k in 0..3 {
                let uvi = f[3 + k] as usize * 2;
                let uv = if uvi + 1 < ech.uvs.len() {
                    [ech.uvs[uvi], ech.uvs[uvi + 1]]
                } else {
                    [0.0, 0.0]
                };
                ecorners.push((f[k], uv));
            }
        }
        enemies.push(EnemySpec {
            ch: ech,
            map: emap,
            corners: ecorners,
            move_cycle: eget(e.clips.0)?,
            pounce: eget(e.clips.1)?,
            hit: eget(e.clips.2)?,
            death: eget(e.clips.3)?,
            spawn: [0.0, 0.0, 0.0], // placed below, near the player
            name: e.name,
            height: e.height,
        });
    }
    // Pole clip set (grip / climb / spin).
    let pole_clips = match preset.pole {
        Some((grip, climb, spin)) => Some(PoleClips {
            grip: load_clip("Dynamic_Actions_Column", grip)?,
            climb: load_clip("Dynamic_Actions_Column", climb)?,
            spin: load_clip("Dynamic_Actions_Column", spin)?,
        }),
        None => None,
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
    // Enemy placement: grid-search around the player's
    // anchor for ground on (nearly) the same floor — the
    // dock is too narrow for fixed offsets. Each enemy
    // must land at least 180 units from the ones already
    // placed.
    {
        // The floor the player will actually stand on: in
        // the anchor fallback, gz is the level's TOP, not a
        // floor — search down the spawn column instead.
        let floor_z = soup.ground_height(sx, sy, gz + 400.0, 30.0).unwrap_or(gz);
        let mut placed: Vec<[f32; 3]> = Vec::new();
        for e in enemies.iter_mut() {
            let mut best: Option<(f32, [f32; 3])> = None;
            for gx in -20..=20 {
                for gy in -20..=20 {
                    let (ex, ey) = (sx + gx as f32 * 30.0, sy + gy as f32 * 30.0);
                    let dx = ex - sx;
                    let dy = ey - sy;
                    let d = (dx * dx + dy * dy).sqrt();
                    if !(55.0..=560.0).contains(&d) {
                        continue;
                    }
                    if placed.iter().any(|p| {
                        let pdx = p[0] - ex;
                        let pdy = p[1] - ey;
                        pdx * pdx + pdy * pdy < 180.0 * 180.0
                    }) {
                        continue;
                    }
                    if let Some(eg) = soup.ground_height(ex, ey, floor_z + 60.0, 30.0) {
                        if (eg - floor_z).abs() < 40.0 {
                            let score = (d - 380.0).abs();
                            if best.map_or(true, |(s, _)| score < s) {
                                best = Some((score, [ex, ey, eg + 2.0]));
                            }
                        }
                    }
                }
            }
            if let Some((_, sp)) = best {
                e.spawn = sp;
                placed.push(sp);
                println!("[t2t] enemy spawn (Jade): {} at {:?}", e.name, e.spawn);
            }
        }
        // An enemy the grid could not place keeps the
        // [0,0,0] sentinel spawn — never spawn it there.
        enemies.retain(|e| {
            let ok = e.spawn != [0.0, 0.0, 0.0];
            if !ok {
                println!("[t2t] enemy NOT placed (no floor found): {}", e.name);
            }
            ok
        });
    }
    Some(CharSpec {
        ch,
        clip,
        clip_air,
        clip_air_key,
        traversal,
        pole_clips,
        clip_attack,
        clip_hit,
        enemies,
        map,
        corners,
        spawn,
        soup,
        center: world.center,
        scale: world.scale,
        bevy_height: preset.height * world.scale,
        name: preset.name,
        clip_key: preset.clip,
        kill_z: world2.aabb().0[2] - 400.0,
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
    mut q_enemy: Query<&mut EnemyChar>,
    mut q_hud: Query<&mut Text, With<HudText>>,
    mut cam: Query<&mut Transform, With<FollowCam>>,
) {
    let dt = time.delta_secs().min(0.05);
    for mut ac in &mut q {
        // Sand power: R spends a tank on slow motion
        // (staging: T2T_SLOWMO_AT=<sec> fires it once).
        // While it runs, the world crawls at 0.3x and the
        // Prince moves at 0.9x — per-actor dt scaling.
        ac.elapsed += dt;
        ac.slowmo_t = (ac.slowmo_t - dt).max(0.0);
        let staged = match std::env::var("T2T_SLOWMO_AT") {
            Ok(v) => {
                !ac.slowmo_staged
                    && v.parse::<f32>().map_or(false, |at| ac.elapsed >= at)
            }
            Err(_) => false,
        };
        if (keys.just_pressed(KeyCode::KeyR) || staged) && ac.slowmo_t <= 0.0 {
            if ac.sand.spend(SLOWMO_COST) {
                ac.slowmo_t = SLOWMO_SECS;
                println!(
                    "[t2t] slow motion engaged (sand {} -> {})",
                    ac.sand.sand + SLOWMO_COST,
                    ac.sand.sand
                );
            } else if staged {
                println!("[t2t] slow-mo staging fired but the tank is empty");
            }
            if staged {
                ac.slowmo_staged = true;
            }
        }
        let pdt = dt
            * if ac.slowmo_t > 0.0 {
                SLOWMO_PLAYER_SCALE
            } else {
                1.0
            };
        // Combat state: tick timers, trigger swings, and
        // lock movement while attacking or reeling.
        ac.fighter.update(pdt, 0.5);
        if spec.clip_attack.is_some()
            && (keys.just_pressed(KeyCode::KeyF) || mouse_buttons.just_pressed(MouseButton::Right))
        {
            ac.fighter.start_attack(0.55);
        }
        let busy = matches!(
            ac.fighter.state,
            FighterState::Attacking { .. } | FighterState::HitReact { .. }
        );
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
        // On a pole, WASD drives the grip instead of
        // walking: W/S climb, A/D orbit; S no longer drops.
        let on_pole = ac.ctl.pole.is_some();
        if busy || on_pole {
            wish = [0.0, 0.0];
        }
        let wl = (wish[0] * wish[0] + wish[1] * wish[1]).sqrt();
        let speed = if keys.pressed(KeyCode::ShiftLeft) { 340.0 } else { 170.0 };
        let pole_move = if on_pole {
            [
                (if keys.pressed(KeyCode::KeyW) { 1.0 } else { 0.0 })
                    + (if keys.pressed(KeyCode::KeyS) { -1.0 } else { 0.0 }),
                (if keys.pressed(KeyCode::KeyA) { 1.0 } else { 0.0 })
                    + (if keys.pressed(KeyCode::KeyD) { -1.0 } else { 0.0 }),
            ]
        } else {
            [0.0, 0.0]
        };
        let input = MoveInput {
            vel_xy: if wl > 0.01 {
                [wish[0] / wl * speed, wish[1] / wl * speed]
            } else {
                [0.0, 0.0]
            },
            jump: keys.just_pressed(KeyCode::Space),
            drop: keys.just_pressed(KeyCode::KeyS) && !on_pole,
            pole_move,
        };
        // Fixed-substep physics for stability.
        let steps = 2;
        for _ in 0..steps {
            ac.ctl.step(&spec.soup, &input, pdt / steps as f32);
        }
        // Checkpoints: a grounded spot 400+ units past
        // the current one becomes the respawn point.
        let (cpos, cyaw, cgrounded) = (ac.ctl.pos, ac.ctl.yaw, ac.ctl.grounded);
        if ac.fighter.alive() && ac.checkpoint.observe(cpos, cyaw, cgrounded) {
            println!("[t2t] checkpoint set at {cpos:?}");
        }
        // Falling out of the world is fatal; in a combat
        // sector the reset below handles the respawn, in
        // a quiet sector it happens right here.
        if ac.ctl.pos[2] < spec.kill_z && ac.fighter.alive() {
            println!("[t2t] the Prince fell out of the world (z {:.0})", ac.ctl.pos[2]);
            ac.fighter.take_hit(999);
        }
        if spec.enemies.is_empty() && ac.fighter.state == FighterState::Dead {
            let rp = ac.checkpoint.respawn_point(spec.spawn);
            println!("[t2t] respawning at {rp:?}");
            ac.ctl = Controller::new(rp);
            ac.ctl.yaw = ac.checkpoint.yaw;
            ac.fighter = player_fighter();
            ac.sand = SandTank::new(SAND_MAX, start_sand());
            ac.slowmo_t = 0.0;
        }

        // Animation time + mesh rebuild. State-driven clip
        // choice: hanging -> traversal set, airborne -> the
        // air clip (own clock), grounded -> the main clip.
        ac.time += pdt;
        let mut fixed_frame: Option<u32> = None;
        // Combat clips take priority over traversal/air.
        let combat_pick: Option<(&Clip, u32)> = match (ac.fighter.state, &spec.clip_attack, &spec.clip_hit) {
            (FighterState::Attacking { t, .. }, Some(atk), _) => {
                let f = (t / ac.fighter.strike.duration * (atk.max_frame + 1) as f32) as u32;
                Some((atk, f.min(atk.max_frame)))
            }
            (FighterState::HitReact { t }, _, Some(hit)) => {
                let f = (t / 0.5 * (hit.max_frame + 1) as f32) as u32;
                Some((hit, f.min(hit.max_frame)))
            }
            _ => None,
        };
        let (active_clip, active_time) =
            if let Some((c, f)) = combat_pick {
                fixed_frame = Some(f);
                (c, 0.0)
            } else if let (Some(pc), Some(_)) = (&spec.pole_clips, ac.ctl.pole) {
                ac.pole_time += pdt;
                if pole_move[0].abs() > 0.01 {
                    (&pc.climb, ac.pole_time)
                } else if pole_move[1].abs() > 0.01 {
                    (&pc.spin, ac.pole_time)
                } else {
                    (&pc.grip, ac.pole_time)
                }
            } else if let (Some(trav), Some(h)) = (&spec.traversal, ac.ctl.hanging) {
                ac.hang_time += pdt;
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
                        ac.air_time += pdt;
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

    // ---- Combat: strikes, the enemy AI, respawn, HUD ----
    if !spec.enemies.is_empty() {
        let mut foes: Vec<Mut<EnemyChar>> = q_enemy.iter_mut().collect();
        let mut hud = String::new();
        // Enemies live on world time: slowed while the
        // Prince's slow motion runs.
        let edt = dt
            * if q.single().map_or(false, |ac| ac.slowmo_t > 0.0) {
                SLOWMO_ENEMY_SCALE
            } else {
                1.0
            };
        for ec in foes.iter_mut() {
            let e_spec = &spec.enemies[ec.idx];
            ec.fighter.update(edt, 0.9);
            ec.time += edt;
            let epos = ec.ctl.pos;
            let (p_pos, _p_yaw) = q
                .single()
                .map(|ac| (ac.ctl.pos, ac.ctl.yaw))
                .unwrap_or((spec.spawn, 0.0));
            // The player's swing, resolved against this
            // enemy (one connection per swing: hit_done is
            // on the attacker).
            if let Ok(mut ac) = q.single_mut() {
                let (pp, py) = (ac.ctl.pos, ac.ctl.yaw);
                ac.fighter.strike_connects(pp, py, epos, &mut ec.fighter);
            }
            let to_player = [p_pos[0] - epos[0], p_pos[1] - epos[1]];
            let dist = (to_player[0] * to_player[0] + to_player[1] * to_player[1]).sqrt();
            let (e_clip, e_frame): (&Clip, u32) = match ec.fighter.state {
                FighterState::Dead => {
                    // A kill feeds the Prince's sand, once.
                    if !ec.sand_paid {
                        ec.sand_paid = true;
                        if let Ok(mut ac) = q.single_mut() {
                            let got = ac.sand.gain(SAND_PER_KILL);
                            println!(
                                "[t2t] {} falls - the Prince absorbs {got} sand (tank {}/{})",
                                e_spec.name, ac.sand.sand, ac.sand.max
                            );
                        }
                    }
                    ec.dead_time += edt;
                    let f = ((ec.dead_time / 1.1).min(1.0) * e_spec.death.max_frame as f32) as u32;
                    (&e_spec.death, f)
                }
                FighterState::HitReact { t } => {
                    let f = (t / 0.9 * (e_spec.hit.max_frame + 1) as f32) as u32;
                    (&e_spec.hit, f.min(e_spec.hit.max_frame))
                }
                FighterState::Attacking { t, .. } => {
                    if let Ok(mut ac) = q.single_mut() {
                        let (pp, ey) = (ac.ctl.pos, ec.ctl.yaw);
                        ec.fighter.strike_connects(epos, ey, pp, &mut ac.fighter);
                    }
                    let f = (t / ec.fighter.strike.duration * (e_spec.pounce.max_frame + 1) as f32) as u32;
                    (&e_spec.pounce, f.min(e_spec.pounce.max_frame))
                }
                FighterState::Ready => {
                    if dist < 1100.0 && dist > 1e-6 {
                        if dist > 215.0 {
                            let v = [
                                to_player[0] / dist * 165.0,
                                to_player[1] / dist * 165.0,
                            ];
                            let inp = MoveInput { vel_xy: v, jump: false, drop: false, pole_move: [0.0, 0.0] };
                            for _ in 0..2 {
                                ec.ctl.step(&spec.soup, &inp, edt / 2.0);
                            }
                        } else {
                            ec.ctl.yaw = to_player[1].atan2(to_player[0]);
                            ec.fighter.start_attack(1.6);
                        }
                    }
                    let f = ((ec.time * 30.0) as u32) % (e_spec.move_cycle.max_frame + 1);
                    (&e_spec.move_cycle, f)
                }
            };
            let (positions, normals) = char_corners(
                &e_spec.ch,
                e_clip,
                &e_spec.map,
                e_frame,
                ec.ctl.pos,
                ec.ctl.yaw,
                spec.center,
                spec.scale,
            );
            if let Some(mesh) = meshes.get_mut(&ec.mesh) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            }
            hud.push_str(&format!(
                "    {} HP {} / {}",
                e_spec.name, ec.fighter.hp, ec.fighter.max_hp
            ));
        }
        // Player death -> reset the whole fight.
        if let Ok(mut ac) = q.single_mut() {
            if ac.fighter.state == FighterState::Dead {
                let rp = ac.checkpoint.respawn_point(spec.spawn);
                if ac.checkpoint.point.is_some() {
                    println!("[t2t] the Prince falls - respawning at checkpoint {rp:?}");
                } else {
                    println!("[t2t] the Prince falls - respawning all fighters");
                }
                ac.ctl = Controller::new(rp);
                ac.ctl.yaw = ac.checkpoint.yaw;
                ac.fighter = player_fighter();
                ac.sand = SandTank::new(SAND_MAX, start_sand());
                ac.slowmo_t = 0.0;
                for ec in foes.iter_mut() {
                    ec.ctl = Controller::new(ec.home);
                    ec.fighter = enemy_fighter();
                    ec.dead_time = 0.0;
                    ec.sand_paid = false;
                }
            }
            if let Ok(mut text) = q_hud.single_mut() {
                let slow = if ac.slowmo_t > 0.0 {
                    format!("    [SLOW-MO {:.1}s]", ac.slowmo_t)
                } else {
                    String::new()
                };
                **text = format!(
                    "Prince HP {} / {}    Sand {} / {}{}{}",
                    ac.fighter.hp, ac.fighter.max_hp, ac.sand.sand, ac.sand.max, hud, slow
                );
            }
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
                pole_time: 0.0,
                ctl: ctl0,
                orbit_yaw: 0.0,
                orbit_pitch: 0.25,
                fighter: player_fighter(),
                sand: SandTank::new(SAND_MAX, start_sand()),
                slowmo_t: 0.0,
                elapsed: 0.0,
                slowmo_staged: false,
                checkpoint: seed_checkpoint(),
            },
        ));
        // Combat opponents + HUD.
        for (idx, e) in spec.enemies.iter().enumerate() {
            let (ep, en) = char_corners(
                &e.ch,
                &e.move_cycle,
                &e.map,
                0,
                e.spawn,
                0.0,
                spec.center,
                spec.scale,
            );
            let en_n = ep.len();
            let mut emesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            emesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, ep);
            emesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, en);
            emesh.insert_attribute(
                Mesh::ATTRIBUTE_UV_0,
                e.corners.iter().map(|c| c.1).collect::<Vec<_>>(),
            );
            emesh.insert_attribute(
                Mesh::ATTRIBUTE_COLOR,
                vec![[0.72f32, 0.62, 0.5, 1.0]; en_n],
            );
            emesh.insert_indices(Indices::U32((0..en_n as u32).collect()));
            let ehandle = meshes.add(emesh);
            println!("[t2t] enemy: {} ({} corners)", e.name, en_n);
            commands.spawn((
                Mesh3d(ehandle.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    ..default()
                })),
                Transform::default(),
                EnemyChar {
                    idx,
                    mesh: ehandle,
                    ctl: Controller::new(e.spawn),
                    fighter: enemy_fighter(),
                    time: 0.0,
                    dead_time: 0.0,
                    home: e.spawn,
                    sand_paid: false,
                },
            ));
        }
        if !spec.enemies.is_empty() {
            commands.spawn((
                Text::new(""),
                TextFont { font_size: 24.0, ..default() },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(14.0),
                    left: Val::Px(14.0),
                    ..default()
                },
                HudText,
            ));
        }
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
