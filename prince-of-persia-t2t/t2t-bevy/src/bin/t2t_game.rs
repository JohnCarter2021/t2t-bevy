//! t2t_game — the Bevy reimplementation entry point.
//!
//! M1: boots Bevy, indexes the original archive.
//! M3: loads a _wow_ world, builds a Bevy mesh from its largest GEO.
//! M4: assembles the WHOLE world — every GAO's geometry transformed
//! by its global matrix into world space, split into one Bevy mesh
//! per texture key with the real decoded texture sampled (the
//! texture-binding chain GAO -> material -> texture is resolved in
//! t2t::assemble; faces with no texture render flat grey).
//! Characters still render flat-tinted: their material chain is
//! not resolved by the character loader yet. The scene is centered
//! on the world AABB and scaled to ~40 units across.
//!
//! Usage: t2t_game [path-to-pop3.bf] [wow-name-substring]
//!   defaults: pop3.bf, "Menu3D"

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::alpha::AlphaMode;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::input::mouse::MouseMotion;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::dare::{decode_sound, decode_sound_from, parse_bank, parse_fat, wav_bytes};
use t2t::collision::TriangleSoup;
use t2t::combat::{Fighter, FighterState, SandTank, StrikeProfile};
use t2t::controller::{Checkpoint, Controller, MoveInput, CLIMB_TIME};
use t2t::gao::parse_gao;
use t2t::geometry::parse_geometry;
use t2t::jade_stream::decompress_entry;
use t2t::material::texture_key_for;
use t2t::skin::{posed_verts, ChannelMap, Character, Clip};
use t2t::texture::{decode_texture_pal, is_texture_entry, mip_chain, palette_key, parse_texture};
use t2t::wow::walk_wow;

#[derive(Resource)]
struct GameArchive {
    file_count: usize,
    dir_count: usize,
    version: u32,
}

/// One texture's worth of the merged world: de-indexed triangles
/// sharing a single texture key (None = no texture resolved).
struct WorldMeshGroup {
    tex_key: Option<u32>,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

/// A decoded world texture, ready to become a Bevy Image.
struct WorldTexture {
    key: u32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    /// True when the texture mixes opaque and transparent pixels:
    /// a cutout (grate, foliage) that needs alpha masking, not
    /// plain opaque rendering.
    cutout: bool,
}

/// De-indexed mesh data ready for Bevy: the merged world split by
/// texture key, plus the Jade->Bevy frame.
#[derive(Resource)]
struct WorldMesh {
    groups: Vec<WorldMeshGroup>,
    textures: Vec<WorldTexture>,
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
    /// Texture atlas for the enemy's skin (None = flat tint).
    atlas: Option<CharAtlas>,
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
    /// Texture atlas for the character's skin (None = flat tint).
    /// Corner UVs are already remapped into it when present.
    atlas: Option<CharAtlas>,
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

/// M7 presentation: the app boots into the main-menu
/// phase (the game's real Menu3D scene as backdrop, an
/// orbiting camera, title UI) and transitions into
/// gameplay on Enter. Staging: T2T_START=play skips the
/// menu entirely; T2T_MENU_SECS=<s> auto-starts after
/// s seconds of menu time.
#[derive(Resource, PartialEq, Eq, Clone, Copy, Debug)]
enum Phase {
    Menu,
    Playing,
    Paused,
    Cine,
}

fn in_menu(phase: Res<Phase>) -> bool {
    *phase == Phase::Menu
}

fn in_playing(phase: Res<Phase>) -> bool {
    *phase == Phase::Playing
}

fn in_paused(phase: Res<Phase>) -> bool {
    *phase == Phase::Paused
}

fn in_cine(phase: Res<Phase>) -> bool {
    *phase == Phase::Cine
}

fn in_menu_or_paused(phase: Res<Phase>) -> bool {
    *phase == Phase::Menu || *phase == Phase::Paused
}

/// Menu text from the original localization file
/// (POPData.bf's pop4pclocal.dat, INI sections). Entries
/// missing here fall back to the same English strings, so
/// the game still menus when run against a bare pop3.bf.
#[derive(Resource, Default)]
struct MenuStrings {
    map: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
}

impl MenuStrings {
    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.map.get(section)?.get(key).map(|s| s.as_str())
    }
    fn txt(&self, section: &str, key: &str, fallback: &str) -> String {
        self.get(section, key)
            .unwrap_or(fallback)
            .replace("||", "\n")
    }
}

fn load_menu_strings(bf_path: &str) -> MenuStrings {
    let mut out = MenuStrings::default();
    let pop_path = std::path::Path::new(bf_path)
        .parent()
        .map(|p| p.join("POPData.bf"));
    let data = match pop_path.as_ref().and_then(|p| std::fs::read(p).ok()) {
        Some(d) => d,
        None => return out,
    };
    // The .dat is the archive's last file and runs to EOF;
    // locate it by its own first line (the container's
    // name table is only reverse-engineered for two
    // entries, so the marker search is the robust path).
    let marker = b"; General information";
    let start = match data
        .windows(marker.len())
        .position(|w| w == marker)
    {
        Some(i) => i,
        None => return out,
    };
    let text = String::from_utf8_lossy(&data[start..]);
    if !text.contains("[P_MainMenu]") {
        return out;
    }
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            out.map
                .entry(section.clone())
                .or_default()
                .insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

/// Settings the options menus can actually change. Only
/// two toggles are wired so far; every other options row
/// is a faithful label with no effect yet.
#[derive(Resource)]
struct Settings {
    music_on: bool,
    interface_on: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            music_on: true,
            interface_on: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuScreen {
    Main,
    Options,
    Graphics,
    Sounds,
    Controls,
    GameOpts,
    Profiles,
    LoadGame,
    Credits,
    ConfirmQuit,
    Pause,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    StartGame,
    ToScreen(MenuScreen),
    Back,
    Resume,
    QuitToMenu,
    QuitApp,
    ToggleMusic,
    ToggleInterface,
    PlayCredits,
    Noop,
}

struct MenuItem {
    label: String,
    action: MenuAction,
}

/// The items of one menu screen, labels from the original
/// localization with identical English fallbacks.
fn menu_items(
    screen: MenuScreen,
    s: &MenuStrings,
    settings: &Settings,
) -> Vec<MenuItem> {
    let t = |sec: &str, key: &str, fb: &str| s.txt(sec, key, fb);
    let back = |s: &MenuStrings| MenuItem {
        label: s.txt("P_Options", "Back", "Back"),
        action: MenuAction::Back,
    };
    let noop = |label: String| MenuItem {
        label,
        action: MenuAction::Noop,
    };
    match screen {
        MenuScreen::Main => vec![
            MenuItem {
                label: t("P_MainMenu", "NewGame", "Start New Game"),
                action: MenuAction::StartGame,
            },
            MenuItem {
                label: t("P_MainMenu", "LoadGame", "Load Saved Game"),
                action: MenuAction::ToScreen(MenuScreen::LoadGame),
            },
            MenuItem {
                label: t("P_MainMenu", "Options", "Options"),
                action: MenuAction::ToScreen(MenuScreen::Options),
            },
            MenuItem {
                label: t("P_MainMenu", "Credits", "Credits"),
                action: MenuAction::PlayCredits,
            },
            MenuItem {
                label: t("P_MainMenu", "Back", "Change Profile"),
                action: MenuAction::ToScreen(MenuScreen::Profiles),
            },
            MenuItem {
                label: t("P_MainMenu", "Quit", "Quit"),
                action: MenuAction::ToScreen(MenuScreen::ConfirmQuit),
            },
        ],
        MenuScreen::Options => vec![
            MenuItem {
                label: t("P_Options", "Video", "Graphics"),
                action: MenuAction::ToScreen(MenuScreen::Graphics),
            },
            MenuItem {
                label: t("P_Options", "Sounds", "Sounds"),
                action: MenuAction::ToScreen(MenuScreen::Sounds),
            },
            MenuItem {
                label: t("P_Options", "Controls", "Controls"),
                action: MenuAction::ToScreen(MenuScreen::Controls),
            },
            MenuItem {
                label: t("P_Options", "Game", "Game"),
                action: MenuAction::ToScreen(MenuScreen::GameOpts),
            },
            back(s),
        ],
        MenuScreen::Graphics => vec![
            noop(t("P_VideoMenu", "Effects", "Effects")),
            noop(t("P_VideoMenu", "Water", "Water Effects")),
            noop(t("P_VideoMenu", "Shadow", "Shadow Effects")),
            noop(t("P_VideoMenu", "Filtering", "Filtering")),
            noop(t("P_VideoMenu", "SpecialFX", "Special Effects")),
            noop(t("P_VideoMenu", "Fog", "Fog")),
            noop(t("P_VideoMenu", "VSync", "V-Sync.")),
            noop(t("P_VideoMenu", "TextureQuality", "Texture Quality")),
            noop(t("P_VideoMenu", "Resolution", "Resolution")),
            noop(t("P_VideoMenu", "Gamma", "Gamma")),
            noop(t("P_VideoMenu", "Brightness", "Brightness")),
            noop(t("P_VideoMenu", "Contrast", "Contrast")),
            noop(t("P_VideoMenu", "Advanced", "Advanced")),
            noop(t("P_VideoMenu", "DefaultSettings", "Default Settings")),
            noop(t("P_VideoMenu", "Save", "Save Settings")),
            back(s),
        ],
        MenuScreen::Sounds => vec![
            MenuItem {
                label: format!(
                    "Music: {}",
                    if settings.music_on {
                        s.txt("P_SoundsMenu", "MusicOn", "On")
                    } else {
                        s.txt("P_SoundsMenu", "MusicOff", "Off")
                    }
                ),
                action: MenuAction::ToggleMusic,
            },
            noop(t("P_SoundsMenu", "MusicVolume", "Music Volume")),
            noop(t("P_SoundsMenu", "VoicesVolume", "Voices Volume")),
            noop(t("P_SoundsMenu", "EffectsVolume", "Effects Volume")),
            noop(t(
                "P_SoundsMenu",
                "Virtualization",
                "Audio Virtualization",
            )),
            noop(t(
                "P_SoundsMenu",
                "3DAudioHardwareAcceleration",
                "3D Audio Hardware Acceleration",
            )),
            noop(t("P_SoundsMenu", "DefaultSettings", "Default Settings")),
            noop(t("P_SoundsMenu", "Save", "Save Settings")),
            back(s),
        ],
        MenuScreen::Controls => vec![
            noop(t("P_ControlsMenu", "Keyboard", "Keyboard")),
            noop(t("P_ControlsMenu", "Gamepad", "Gamepad")),
            noop(t("P_ControlsMenu", "Camera", "Camera")),
            noop(t("P_ControlsMenu", "MouseSpeed", "Mouse speed")),
            noop(t("P_ControlsMenu", "FreeLook", "Camera Freelook")),
            noop(t("P_ControlsMenu", "Navigation", "Camera Navigation")),
            noop(t("P_ControlsMenu", "DefaultSettings", "Default Settings")),
            noop(t("P_ControlsMenu", "Save", "Save Settings")),
            back(s),
        ],
        MenuScreen::GameOpts => vec![
            noop(t("P_GameOptions", "InsertFrequency", "Insert Frequency")),
            noop(t("P_GameOptions", "Tutorials", "Tutorials")),
            noop(t("P_GameOptions", "SetupMenu", "Setup Menu")),
            noop(t("P_GameOptions", "Cheats", "Cheats")),
            MenuItem {
                label: format!(
                    "{}: {}",
                    t("P_GameOptions", "Interface", "Interface"),
                    if settings.interface_on {
                        s.txt("P_GameOptions", "InterfaceOn", "On")
                    } else {
                        s.txt("P_GameOptions", "InterfaceOff", "Off")
                    }
                ),
                action: MenuAction::ToggleInterface,
            },
            noop(t("P_GameOptions", "WorkInProgress", "Work in Progress")),
            noop(t("P_GameOptions", "DefaultSettings", "Default Settings")),
            noop(t("P_GameOptions", "Save", "Save Settings")),
            back(s),
        ],
        MenuScreen::Profiles => vec![
            noop(t("P_ProfileMenu", "CreateNew", "New Profile")),
            back(s),
        ],
        MenuScreen::LoadGame => vec![back(s)],
        MenuScreen::Credits => vec![back(s)],
        MenuScreen::ConfirmQuit => vec![
            MenuItem {
                label: t("ConfirmQuit", "Yes", "Yes"),
                action: MenuAction::QuitApp,
            },
            MenuItem {
                label: t("ConfirmQuit", "No", "No"),
                action: MenuAction::Back,
            },
        ],
        MenuScreen::Pause => vec![
            MenuItem {
                label: t("P_InGameMenu", "Continue", "Continue"),
                action: MenuAction::Resume,
            },
            MenuItem {
                label: t("P_InGameMenu", "Options", "Options"),
                action: MenuAction::ToScreen(MenuScreen::Options),
            },
            MenuItem {
                label: t("P_InGameMenu", "MainMenu", "Quit to Main Menu"),
                action: MenuAction::QuitToMenu,
            },
            MenuItem {
                label: t("P_InGameMenu", "Back", "Back"),
                action: MenuAction::Resume,
            },
        ],
    }
}

/// Screen title + info line shown above the list. The
/// info lines for Load/Credits are this build's own state
/// text, not original strings.
fn menu_screen_text(screen: MenuScreen, s: &MenuStrings) -> (String, String) {
    match screen {
        MenuScreen::Main => (String::new(), String::new()),
        MenuScreen::Options => (s.txt("P_Options", "Title", "Options"), String::new()),
        MenuScreen::Graphics => (s.txt("P_VideoMenu", "Title", "Graphics"), String::new()),
        MenuScreen::Sounds => (s.txt("P_SoundsMenu", "Title", "Sounds"), String::new()),
        MenuScreen::Controls => (s.txt("P_ControlsMenu", "Title", "Controls"), String::new()),
        MenuScreen::GameOpts => (
            s.txt("P_GameOptions", "Title", "Game Options"),
            String::new(),
        ),
        MenuScreen::Profiles => (
            s.txt("P_ProfileMenu", "Title", "Choose Your Profile"),
            String::new(),
        ),
        MenuScreen::LoadGame => (
            s.txt("P_MainMenu", "LoadGame", "Load Saved Game"),
            "No saved games yet".to_string(),
        ),
        MenuScreen::Credits => ("Credits".to_string(), "Credits video not wired yet".to_string()),
        MenuScreen::ConfirmQuit => (
            String::new(),
            s.txt(
                "ConfirmQuit",
                "Text",
                "Are you sure you want to quit\nPrince of Persia?",
            ),
        ),
        MenuScreen::Pause => ("Paused".to_string(), String::new()),
    }
}

/// Entities that belong to the menu presentation.
#[derive(Component)]
struct MenuEntity;

/// Entities that belong to the gameplay scene.
#[derive(Component)]
struct GameEntity;

/// The Menu3D world prepared separately from the game
/// sector (None when the game sector IS Menu3D — its
/// world doubles as the menu backdrop).
#[derive(Resource)]
struct MenuWorld(Option<WorldMesh>);

/// Menu-phase bookkeeping: orbit clock, auto-start
/// timer, and whether a playable character exists.
#[derive(Resource)]
struct MenuState {
    t: f32,
    auto_secs: Option<f32>,
    has_char: bool,
    screen: MenuScreen,
    /// Screens to return to on Back (Main/Pause at root).
    stack: Vec<MenuScreen>,
    selected: usize,
    /// Last rendered (screen, selected, music, interface)
    /// for the menu list; the pause list tracks its own.
    built: Option<(MenuScreen, usize, bool, bool)>,
    pause_built: Option<(MenuScreen, usize, bool, bool)>,
}

/// The menu item list container (children rebuilt on
/// screen/selection change).
#[derive(Component)]
struct MenuList;

/// The info line above the menu list (screen titles,
/// confirm questions, empty-state notes).
#[derive(Component)]
struct MenuInfo;

/// The pause overlay root (visible only while Paused).
#[derive(Component)]
struct PauseUi;

/// The info line in the pause overlay (sub-screen titles).
#[derive(Component)]
struct PauseInfo;

/// The pause item list container.
#[derive(Component)]
struct PauseList;

/// The in-game HUD bar root (health + sand segments).
#[derive(Component)]
struct HudRoot;

/// One HUD bar segment. A single component (not separate
/// Hp/Sand markers) because Bevy rejects two &mut queries
/// over the same component in one system (B0001).
#[derive(Clone, Copy, PartialEq, Eq)]
enum HudSegKind {
    Hp,
    Sand,
}

#[derive(Component)]
struct HudSeg {
    kind: HudSegKind,
    idx: usize,
}

/// M7 audio: DARE sounds decoded at startup (prepare
/// step in main), carried as in-memory WAV bytes. The
/// picks are grounded in the game's own event data
/// (usage/t2t.tsv of PopAudioStudio + the bank event
/// maps): the menu music is stream event 0x700000
/// (64.0 s stereo @ 24000 Hz, referenced by
/// MenuInGameAI / Menu_Boot), the UI sounds are bank
/// 002's Menu_Boot events 0x190002 (boot) and
/// 0x190007 (confirm).
#[derive(Resource)]
struct GameAudio {
    music_wav: Vec<u8>,
    ui_boot_wav: Vec<u8>,
    ui_confirm_wav: Vec<u8>,
    /// Sword-impact variants: bank 003 event
    /// 0x170024 — the blockSound every weapon object
    /// references (container of 3 waves).
    block_wavs: Vec<Vec<u8>>,
    /// Flesh-hit sound: bank 003 event 0x17000f
    /// (Impact_Hurt — identified by elimination
    /// against the three Enemies_Weapon impact
    /// functions plus its dull-thud waveform; the
    /// third, 0x170010 GroundAndWall, has no
    /// faithful trigger in this combat model and
    /// is deliberately NOT loaded).
    hurt_wav: Option<Vec<u8>>,
}

/// Bevy audio asset handles built from GameAudio in
/// setup.
#[derive(Resource)]
struct AudioHandles {
    music: Handle<AudioSource>,
    ui_boot: Handle<AudioSource>,
    ui_confirm: Handle<AudioSource>,
    /// Reserved for a future parry/block state: this
    /// combat model has no blocking, so strikes play
    /// the hurt sound, not the block variants.
    #[allow(dead_code)]
    block: Vec<Handle<AudioSource>>,
    hurt: Option<Handle<AudioSource>>,
}

/// The looping menu-music player entity (despawned
/// on the menu -> playing transition).
#[derive(Component)]
struct MusicPlayer;

/// One Bink intro clip, pre-extracted to JPEG frames
/// + a WAV (ffmpeg decodes Bink 1 natively; the cache
/// lives in out/video_cache — see prepare_intro).
struct IntroClip {
    name: String,
    frames: Vec<Vec<u8>>,
    audio_wav: Option<Vec<u8>>,
}

/// Boot intro state: plays the publisher/title clips
/// (UBISOFT -> UBIMC -> LOGORP -> GO_GEN, 29.97 fps)
/// over the menu, then hands over to the menu (music
/// + boot sound start at the handover). Enter, Space
/// or a click skips the current clip.
#[derive(Resource)]
struct IntroState {
    active: bool,
    clips: Vec<IntroClip>,
    clip: usize,
    t: f32,
    shown: usize,
    started: bool,
    img: Option<Handle<Image>>,
    clip_audio: Vec<Option<Handle<AudioSource>>>,
}

/// The fullscreen intro video node.
#[derive(Component)]
struct IntroNode;

/// In-game story-clip player: the seven Video/Cine/P3C_* clips
/// (640x448 @ 29.97, same frame-streaming tech as the boot
/// intro). Gameplay systems are gated on Phase::Playing, so
/// entering Phase::Cine suspends the game; on end (or skip)
/// the phase returns to `return_to` and gameplay resumes.
/// Staging: T2T_CINE=<name|all> loads the clip(s) from the
/// video cache (Cine/<name>/ frames + Cine/<name>.wav);
/// without T2T_CINE_AT it plays at boot, with
/// T2T_CINE_AT=<secs> it interrupts gameplay after that many
/// seconds of Playing time. Real story-beat triggers belong
/// to M8 campaign work — this is the player + trigger
/// mechanism they will call. The CREDITS.BIK clip
/// (800x600, loaded from cache Credits/ whenever present)
/// rides the same player: the Credits menu item (or the
/// T2T_PLAY_CREDITS staging env) requests it, the player
/// plays just that clip (stop_after) and returns to Menu.
/// The node image is resized on the first frame of a
/// differently-sized clip, so 800x600 credits frames play
/// through the 640x448-sized node.
#[derive(Resource)]
struct CineState {
    active: bool,
    pending: bool,
    at_secs: Option<f32>,
    elapsed: f32,
    return_to: Phase,
    clips: Vec<IntroClip>,
    credits_idx: Option<usize>,
    stop_after: Option<usize>,
    clip: usize,
    t: f32,
    shown: usize,
    started: bool,
    img: Option<Handle<Image>>,
    clip_audio: Vec<Option<Handle<AudioSource>>>,
    audio_entity: Option<Entity>,
}

/// Marker resource: the Credits menu item (or the
/// T2T_PLAY_CREDITS staging env) asks the Cine player to
/// play the credits clip. cine_trigger consumes it.
#[derive(Resource)]
struct CreditsRequest;

/// The fullscreen Cine video node.
#[derive(Component)]
struct CineNode;

/// Load the staged Cine clip(s) from the video cache
/// (T2T_VIDEO_CACHE, default out/video_cache), plus the
/// credits clip from cache Credits/ whenever present.
/// Returns None when neither exists — the game then runs
/// with no Cine support. A missing staged clip is skipped
/// with a warning (it must not kill the credits clip).
fn prepare_cine() -> Option<CineState> {
    let dir = std::env::var("T2T_VIDEO_CACHE").unwrap_or_else(|_| "out/video_cache".to_string());
    let base = std::path::Path::new(&dir).join("Cine");
    let mut clips = Vec::new();
    if let Ok(want) = std::env::var("T2T_CINE") {
        let names: Vec<String> = if want == "all" {
            ["P3C_010", "P3C_020", "P3C_030", "P3C_040", "P3C_050", "P3C_060", "P3C_070"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            vec![want]
        };
        for name in &names {
            let mut frames: Vec<(String, Vec<u8>)> = Vec::new();
            for e in std::fs::read_dir(base.join(name)).into_iter().flatten().flatten() {
                let p = e.path();
                if p.extension().map_or(false, |x| x == "jpg") {
                    if let Ok(b) = std::fs::read(&p) {
                        frames.push((p.file_name().unwrap().to_string_lossy().into_owned(), b));
                    }
                }
            }
            frames.sort();
            if frames.is_empty() {
                eprintln!("[t2t] cine: no frames for {name} in cache — skipping");
                continue;
            }
            clips.push(IntroClip {
                name: name.clone(),
                frames: frames.into_iter().map(|(_, b)| b).collect(),
                audio_wav: std::fs::read(base.join(format!("{name}.wav"))).ok(),
            });
        }
    }
    // Credits clip: cache Credits/ frames + Credits.wav.
    let mut credits_idx = None;
    {
        let cbase = std::path::Path::new(&dir).join("Credits");
        let mut frames: Vec<(String, Vec<u8>)> = Vec::new();
        for e in std::fs::read_dir(&cbase).into_iter().flatten().flatten() {
            let p = e.path();
            if p.extension().map_or(false, |x| x == "jpg") {
                if let Ok(b) = std::fs::read(&p) {
                    frames.push((p.file_name().unwrap().to_string_lossy().into_owned(), b));
                }
            }
        }
        frames.sort();
        if !frames.is_empty() {
            println!("[t2t] cine ready: credits clip, {} frames", frames.len());
            credits_idx = Some(clips.len());
            clips.push(IntroClip {
                name: "CREDITS".to_string(),
                frames: frames.into_iter().map(|(_, b)| b).collect(),
                audio_wav: std::fs::read(cbase.join("Credits.wav")).ok(),
            });
        }
    }
    if clips.is_empty() {
        return None;
    }
    println!(
        "[t2t] cine ready: {} clips, {} frames",
        clips.len(),
        clips.iter().map(|c| c.frames.len()).sum::<usize>()
    );
    let at_secs = std::env::var("T2T_CINE_AT").ok().and_then(|v| v.parse::<f32>().ok());
    // Boot-play only when staged clips exist; a
    // credits-only state must NOT auto-play credits.
    // (Credits is always appended last, so index 0
    // with length 1 means credits-only.)
    let staged_exist = !(credits_idx == Some(0) && clips.len() == 1);
    Some(CineState {
        active: false,
        pending: at_secs.is_none() && staged_exist,
        at_secs,
        elapsed: 0.0,
        return_to: Phase::Menu,
        clips,
        credits_idx,
        stop_after: None,
        clip: 0,
        t: 0.0,
        shown: usize::MAX,
        started: false,
        img: None,
        clip_audio: Vec::new(),
        audio_entity: None,
    })
}

/// Load the extracted intro clips from the video
/// cache (T2T_VIDEO_CACHE, default out/video_cache).
/// Returns None when the cache is missing or the
/// intro is staged off (T2T_START=play, T2T_NO_INTRO)
/// — the game then boots straight into the menu.
fn prepare_intro(start_playing: bool) -> Option<IntroState> {
    if start_playing || std::env::var_os("T2T_NO_INTRO").is_some() {
        return None;
    }
    let dir = std::env::var("T2T_VIDEO_CACHE").unwrap_or_else(|_| "out/video_cache".to_string());
    let base = std::path::Path::new(&dir);
    let mut clips = Vec::new();
    for name in ["UBISOFT", "UBIMC", "LOGORP", "GO_GEN"] {
        let mut frames: Vec<(String, Vec<u8>)> = Vec::new();
        for e in std::fs::read_dir(base.join(name)).ok()?.flatten() {
            let p = e.path();
            if p.extension().map_or(false, |x| x == "jpg") {
                if let Ok(b) = std::fs::read(&p) {
                    frames.push((p.file_name().unwrap().to_string_lossy().into_owned(), b));
                }
            }
        }
        frames.sort();
        if frames.is_empty() {
            return None;
        }
        clips.push(IntroClip {
            name: name.to_string(),
            frames: frames.into_iter().map(|(_, b)| b).collect(),
            audio_wav: std::fs::read(base.join(format!("{name}.wav"))).ok(),
        });
    }
    println!(
        "[t2t] intro ready: {} clips, {} frames",
        clips.len(),
        clips.iter().map(|c| c.frames.len()).sum::<usize>()
    );
    Some(IntroState {
        active: true,
        clips,
        clip: 0,
        t: 0.0,
        shown: usize::MAX,
        started: false,
        img: None,
        clip_audio: Vec::new(),
    })
}

/// Decode the game's menu audio from the Sound
/// directory next to pop3.bf. Any failure disables
/// audio quietly — the game must boot regardless.
fn prepare_audio(bf_path: &str) -> Option<GameAudio> {
    use std::io::{Read, Seek, SeekFrom};
    let sound_dir = std::path::Path::new(bf_path).parent()?.join("Sound");
    let fat = std::fs::read(sound_dir.join("sound.fat")).ok()?;
    let entries = parse_fat(&fat);
    let mut big = std::fs::File::open(sound_dir.join("sound.big")).ok()?;
    let read_entry = |big: &mut std::fs::File, name: &str| -> Option<Vec<u8>> {
        let e = entries.iter().find(|x| x.name == name)?;
        let mut buf = vec![0u8; e.size as usize];
        big.seek(SeekFrom::Start(e.big_offset as u64)).ok()?;
        big.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    // Menu UI sounds: bank 002, via its TableA events.
    let bank002 = read_entry(&mut big, "sound/002.sb0")?;
    let b2 = parse_bank(&bank002).ok()?;
    let ui_wav = |event: u32| -> Option<Vec<u8>> {
        let (_, tb) = b2.events.iter().find(|(id, _)| *id == event)?;
        let s = b2.sounds.iter().find(|s| s.index == *tb)?;
        let pcm = decode_sound(&bank002, s).ok()?;
        Some(wav_bytes(&pcm, s.channels, s.rate))
    };
    let ui_boot_wav = ui_wav(0x190002)?;
    let ui_confirm_wav = ui_wav(0x190007)?;
    // Combat: bank 003's sword blockSound variants.
    let block_wavs = (|| -> Option<Vec<Vec<u8>>> {
        let bank003 = read_entry(&mut big, "sound/003.sb0")?;
        let b3 = parse_bank(&bank003).ok()?;
        let mut out = Vec::new();
        for s in b3.resolve_event(0x170024) {
            let pcm = decode_sound(&bank003, s).ok()?;
            out.push(wav_bytes(&pcm, s.channels, s.rate));
        }
        Some(out)
    })()
    .unwrap_or_default();
    // Combat: bank 003's Impact_Hurt single wave.
    let hurt_wav = (|| -> Option<Vec<u8>> {
        let bank003 = read_entry(&mut big, "sound/003.sb0")?;
        let b3 = parse_bank(&bank003).ok()?;
        let s = b3.resolve_event(0x17000f).into_iter().next()?;
        let pcm = decode_sound(&bank003, s).ok()?;
        Some(wav_bytes(&pcm, s.channels, s.rate))
    })();
    // Menu music: bank 000's stream track 0x700000.
    let bank000 = read_entry(&mut big, "sound/000.sb0")?;
    let b0 = parse_bank(&bank000).ok()?;
    let track = b0.sounds.iter().find(|s| s.id == 0x700000)?;
    let soff = track.stream_offset?;
    let stream = entries.iter().find(|x| x.name == "sound/stream.ss0")?;
    let mut sec = vec![0u8; (track.sec1_len + track.sec2_len) as usize];
    big.seek(SeekFrom::Start(stream.big_offset as u64 + soff as u64))
        .ok()?;
    big.read_exact(&mut sec).ok()?;
    let pcm = decode_sound_from(&sec, track).ok()?;
    let music_wav = wav_bytes(&pcm, track.channels, track.rate);
    println!(
        "[t2t] audio ready: menu music event 0x700000 ({:.1}s, {} ch, {} Hz) + UI events 0x190002/0x190007",
        pcm.len() as f32 / track.channels as f32 / track.rate as f32,
        track.channels,
        track.rate
    );
    Some(GameAudio { music_wav, ui_boot_wav, ui_confirm_wav, block_wavs, hurt_wav })
}

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

/// A sector-exit portal, from its PORTAL GAO: the name
/// fragment after "PORTAL_" is the destination sector (the
/// payload carries no target key — M8 step 3), and the
/// matrix translation is the trigger centre in Jade units.
/// Detection is a 12-unit sphere around that centre (the
/// payload's local box is a few units across; the radius
/// is a margin, not the box itself).
struct Portal {
    name: String,
    target: String,
    pos: [f32; 3],
    fired: bool,
}

#[derive(Resource, Default)]
struct Portals(Vec<Portal>);

fn load_portals(bf_path: &str, wow_name: &str) -> Vec<Portal> {
    let mut out = Vec::new();
    let Ok(bf) = BigFile::open(bf_path) else {
        return out;
    };
    let Some(entry) = bf.active_files().into_iter().find(|e| e.name.contains(wow_name)) else {
        return out;
    };
    let Ok(data) = decompress_entry(bf.read_data(entry.index)) else {
        return out;
    };
    for r in walk_wow(&data) {
        if r.tag != ".gao" {
            continue;
        }
        let range = r.data_range();
        let Some(g) = parse_gao(&data[range]) else {
            continue;
        };
        if !g.name.contains("PORTAL") {
            continue;
        }
        let Some(m) = g.global_matrix else {
            continue;
        };
        let target = g
            .name
            .split("PORTAL_")
            .nth(1)
            .unwrap_or("")
            .trim_end_matches(".gao")
            .split('@')
            .next()
            .unwrap_or("")
            .to_string();
        out.push(Portal {
            name: g.name.clone(),
            target,
            pos: [m[12], m[13], m[14]],
            fired: false,
        });
    }
    for p in &out {
        println!("[t2t] portal: {} -> {} at {:?}", p.name, p.target, p.pos);
    }
    out
}

/// M8: level-placed checkpoints can sit below the floor
/// (Depot's checkpoint is 17.8 under its floor at 74.8) —
/// respawning there falls straight through the world.
/// Snap the respawn point UP to the floor when the point
/// is not already grounded and a floor exists within 40
/// above it; otherwise leave it untouched (a grounded
/// checkpoint is unchanged, and a void respawn still
/// falls as before).
fn snap_respawn(soup: &TriangleSoup, p: [f32; 3]) -> [f32; 3] {
    let grounded = soup
        .ground_height(p[0], p[1], p[2], 0.0)
        .map_or(false, |z| (z - p[2]).abs() < 2.0);
    if grounded {
        return p;
    }
    if let Some(z) = soup.ground_height(p[0], p[1], p[2] + 40.0, 0.0) {
        if z > p[2] {
            return [p[0], p[1], z + 2.0];
        }
    }
    p
}

/// M8: log once when the player walks into a portal's
/// trigger sphere. Travel itself (sector swap) is the
/// next step; this verifies positions + detection first.
fn portal_watch(mut portals: ResMut<Portals>, q: Query<&AnimatedChar>) {
    let Ok(ac) = q.single() else {
        return;
    };
    for p in portals.0.iter_mut().filter(|p| !p.fired) {
        let dx = ac.ctl.pos[0] - p.pos[0];
        let dy = ac.ctl.pos[1] - p.pos[1];
        let dz = ac.ctl.pos[2] - p.pos[2];
        if dx * dx + dy * dy + dz * dz < 12.0 * 12.0 {
            p.fired = true;
            println!(
                "[t2t] portal: entered {} -> {} at {:?}",
                p.name, p.target, ac.ctl.pos
            );
        }
    }
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
    let (mn, mx) = world.aabb();
    let center = [
        (mn[0] + mx[0]) / 2.0,
        (mn[1] + mx[1]) / 2.0,
        (mn[2] + mx[2]) / 2.0,
    ];
    let extent = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]).max(1e-6);
    let s = 40.0 / extent;

    // Split faces by resolved texture key: one Bevy mesh +
    // material per texture (plus one untextured group). The
    // texture decode runs once per USED key, not per face.
    let mut groups: Vec<WorldMeshGroup> = Vec::new();
    let mut group_of: std::collections::HashMap<Option<u32>, usize> =
        std::collections::HashMap::new();
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
            let tk = placed
                .element_textures
                .get(f[6] as usize)
                .copied()
                .flatten();
            let gi = *group_of.entry(tk).or_insert_with(|| {
                groups.push(WorldMeshGroup {
                    tex_key: tk,
                    positions: Vec::new(),
                    normals: Vec::new(),
                    uvs: Vec::new(),
                    indices: Vec::new(),
                });
                groups.len() - 1
            });
            let g = &mut groups[gi];
            for k in 0..3 {
                g.positions.push(vpos(f[k]));
                g.normals.push(n);
                let uvi = f[3 + k] as usize * 2;
                if uvi + 1 < geo.uvs.len() {
                    g.uvs.push([geo.uvs[uvi], geo.uvs[uvi + 1]]);
                } else {
                    g.uvs.push([0.0, 0.0]);
                }
                g.indices.push(g.indices.len() as u32);
            }
        }
    }
    // Decode each used texture once; a decode that does not
    // yield exactly w*h*4 bytes (or a missing record) leaves
    // the group to render in flat grey at spawn time.
    // Palette records for this sector: 1024-byte non-texture
    // records, last occurrence wins.
    let mut pal_map: std::collections::HashMap<u32, std::ops::Range<usize>> =
        std::collections::HashMap::new();
    for r in walk_wow(&data) {
        let sub = &data[r.data_range()];
        if sub.len() == 1024 && !is_texture_entry(sub) {
            pal_map.insert(r.key, r.data_range());
        }
    }
    let mut textures = Vec::new();
    for g in &groups {
        if let Some(key) = g.tex_key {
            if textures.iter().any(|t: &WorldTexture| t.key == key) {
                continue;
            }
            if let Some((ti, range)) = world.textures.get(&key) {
                let tex_data = &data[range.clone()];
                let pal = palette_key(tex_data, ti)
                    .and_then(|pk| pal_map.get(&pk))
                    .map(|pr| &data[pr.clone()]);
                let rgba = decode_texture_pal(tex_data, ti, pal);
                if rgba.len() == ti.width as usize * ti.height as usize * 4
                    && ti.width > 0
                    && ti.height > 0
                {
                    let mut opaque = false;
                    let mut transparent = false;
                    for px in rgba.chunks_exact(4) {
                        if px[3] > 250 {
                            opaque = true;
                        } else if px[3] < 128 {
                            transparent = true;
                        }
                    }
                    textures.push(WorldTexture {
                        key,
                        width: ti.width,
                        height: ti.height,
                        rgba,
                        cutout: opaque && transparent,
                    });
                }
            }
        }
    }
    println!(
        "[t2t] world textures: {} groups, {} decoded, {} cutout",
        groups.len(),
        textures.len(),
        textures.iter().filter(|t| t.cutout).count()
    );
    Some(WorldMesh {
        groups,
        textures,
        tris,
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

/// A character's textures stacked vertically into one atlas image.
/// Characters carry only 1-3 textures, so a single-column atlas
/// keeps the one-mesh-per-character render path untouched: only
/// the corners' UVs get remapped (face order preserved).
struct CharAtlas {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    cutout: bool,
}

/// Resolve a skinned GEO's per-element texture keys and decoded
/// textures from a sector's records: the owning GAO (gro_key ==
/// geo, or a group record naming it) supplies the grm chain, and
/// texture records resolve by LAST occurrence — sectors carry a
/// 56-byte stub before the full record. Returns (per-element
/// texture keys, decoded (key, w, h, rgba) textures).
fn char_textures(
    sector_data: &[u8],
    geo_key: u32,
) -> Option<(Vec<Option<u32>>, Vec<(u32, u32, u32, Vec<u8>)>)> {
    let records = walk_wow(sector_data);
    let mat_view: std::collections::HashMap<u32, (u32, &[u8])> = records
        .iter()
        .filter_map(|r| r.gro_type.map(|g| (r.key, (g, &sector_data[r.data_range()]))))
        .collect();
    let geo_rec = records.iter().find(|r| r.key == geo_key)?;
    let geo = parse_geometry(&sector_data[geo_rec.data_range()]).ok()?;
    let mut grm = None;
    for r in &records {
        if r.tag != ".gao" {
            continue;
        }
        let Some(gao) = parse_gao(&sector_data[r.data_range()]) else {
            continue;
        };
        if gao.grm_key == 0 {
            continue;
        }
        if gao.gro_key == geo_key {
            grm = Some(gao.grm_key);
            break;
        }
        // Group record: the gro_key's own payload names its members.
        if let Some((_, gdata)) = mat_view.get(&gao.gro_key) {
            let mut found = false;
            let mut o = 0;
            while o + 4 <= gdata.len() {
                if u32::from_le_bytes([gdata[o], gdata[o + 1], gdata[o + 2], gdata[o + 3]])
                    == geo_key
                {
                    found = true;
                    break;
                }
                o += 1;
            }
            if found {
                grm = Some(gao.grm_key);
                break;
            }
        }
    }
    let grm = grm?;
    let elem_tex: Vec<Option<u32>> = geo
        .elements
        .iter()
        .map(|(_, mat_id)| texture_key_for(grm, *mat_id, &mat_view))
        .collect();
    // Texture records, last occurrence wins (stub, then full).
    let mut tex_map: std::collections::HashMap<u32, (t2t::texture::TexInfo, std::ops::Range<usize>)> =
        std::collections::HashMap::new();
    for r in &records {
        if let Some(ti) = parse_texture(&sector_data[r.data_range()]) {
            tex_map.insert(r.key, (ti, r.data_range()));
        }
    }
    // Palette records for this sector: 1024-byte non-texture
    // records, last occurrence wins.
    let mut pal_map: std::collections::HashMap<u32, std::ops::Range<usize>> =
        std::collections::HashMap::new();
    for r in &records {
        let sub = &sector_data[r.data_range()];
        if sub.len() == 1024 && !is_texture_entry(sub) {
            pal_map.insert(r.key, r.data_range());
        }
    }
    let mut decoded = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tk in elem_tex.iter().flatten() {
        if !seen.insert(*tk) {
            continue;
        }
        if let Some((ti, range)) = tex_map.get(tk) {
            let tex_data = &sector_data[range.clone()];
            let pal = palette_key(tex_data, ti)
                .and_then(|pk| pal_map.get(&pk))
                .map(|pr| &sector_data[pr.clone()]);
            let mut rgba = decode_texture_pal(tex_data, ti, pal);
            // Characters are solid in-game: force opaque so a
            // palette whose 4th byte is not alpha (Sand Garde /
            // Human Guard) cannot turn the fighter ghostly, and
            // so the atlas stays Opaque as before.
            for px in rgba.chunks_exact_mut(4) {
                px[3] = 255;
            }
            if rgba.len() == ti.width as usize * ti.height as usize * 4
                && ti.width > 0
                && ti.height > 0
            {
                decoded.push((*tk, ti.width, ti.height, rgba));
            }
        }
    }
    Some((elem_tex, decoded))
}

/// Stack decoded textures into one atlas (single column, key
/// order). Returns the atlas plus each key's rect (x, y, w, h).
/// PAL8 textures arrive decoded with their sector palette where
/// one resolves, greyscale where none does.
fn build_char_atlas(
    decoded: &[(u32, u32, u32, Vec<u8>)],
) -> Option<(CharAtlas, std::collections::HashMap<u32, (u32, u32, u32, u32)>)> {
    if decoded.is_empty() {
        return None;
    }
    let mut sorted: Vec<(u32, u32, u32, Vec<u8>)> = decoded.to_vec();
    sorted.sort_by_key(|(k, _, _, _)| *k);
    let width = sorted.iter().map(|(_, w, _, _)| *w).max()?;
    let height: u32 = sorted.iter().map(|(_, _, h, _)| *h).sum();
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    let mut rects = std::collections::HashMap::new();
    let mut y = 0u32;
    for (key, w, h, px) in &sorted {
        for row in 0..*h as usize {
            let dst = ((y as usize + row) * width as usize) * 4;
            let src = row * *w as usize * 4;
            rgba[dst..dst + *w as usize * 4]
                .copy_from_slice(&px[src..src + *w as usize * 4]);
        }
        rects.insert(*key, (0, y, *w, *h));
        y += *h;
    }
    let mut opaque = false;
    let mut transparent = false;
    for px in rgba.chunks_exact(4) {
        if px[3] > 250 {
            opaque = true;
        } else if px[3] < 128 {
            transparent = true;
        }
    }
    Some((
        CharAtlas {
            width,
            height,
            rgba,
            cutout: opaque && transparent,
        },
        rects,
    ))
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
        // Same atlas treatment as the player skin.
        let mut eatlas = None;
        if let Some((eelem_tex, edecoded)) =
            char_textures(&rig_data, e.geo).or_else(|| char_textures(&edata, e.geo))
        {
            if let Some((eat, erect)) = build_char_atlas(&edecoded) {
                for (fi, f) in ech.faces.iter().enumerate() {
                    let tk = eelem_tex.get(f[6] as usize).copied().flatten();
                    if let Some((rx, ry, rw, rh)) = tk.and_then(|k| erect.get(&k)) {
                        for k in 0..3 {
                            let c = &mut ecorners[fi * 3 + k];
                            c.1 = [
                                (*rx as f32 + c.1[0] * *rw as f32) / eat.width as f32,
                                (*ry as f32 + c.1[1] * *rh as f32) / eat.height as f32,
                            ];
                        }
                    }
                }
                println!(
                    "[t2t] character textures: {} ({} keys -> atlas {}x{})",
                    e.name,
                    edecoded.len(),
                    eat.width,
                    eat.height
                );
                eatlas = Some(eat);
            }
        }
        enemies.push(EnemySpec {
            ch: ech,
            map: emap,
            corners: ecorners,
            atlas: eatlas,
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
    // Texture atlas: resolve the skin's element textures, stack
    // them, and remap the corners' UVs into the atlas (face
    // order preserved, so skinned positions stay aligned).
    let mut atlas = None;
    if let Some((elem_tex, decoded)) =
        char_textures(&rig_data, preset.geo).or_else(|| char_textures(&data, preset.geo))
    {
        if let Some((at, rects)) = build_char_atlas(&decoded) {
            for (fi, f) in ch.faces.iter().enumerate() {
                let tk = elem_tex.get(f[6] as usize).copied().flatten();
                if let Some((rx, ry, rw, rh)) = tk.and_then(|k| rects.get(&k)) {
                    for k in 0..3 {
                        let c = &mut corners[fi * 3 + k];
                        c.1 = [
                            (*rx as f32 + c.1[0] * *rw as f32) / at.width as f32,
                            (*ry as f32 + c.1[1] * *rh as f32) / at.height as f32,
                        ];
                    }
                }
            }
            println!(
                "[t2t] character textures: {} ({} keys -> atlas {}x{})",
                preset.name,
                decoded.len(),
                at.width,
                at.height
            );
            atlas = Some(at);
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
        atlas,
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
#[allow(clippy::too_many_arguments)]
fn animate_char(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    spec: Res<CharSpec>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<&mut AnimatedChar>,
    mut q_enemy: Query<&mut EnemyChar>,
    mut cam: Query<&mut Transform, With<FollowCam>>,
    audio_handles: Option<Res<AudioHandles>>,
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
            let rp = snap_respawn(&spec.soup, ac.checkpoint.respawn_point(spec.spawn));
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
        // A weapon strike landing this frame (either
        // direction) plays the blockSound (bank 003
        // event 0x170024, a random variant).
        let mut landed = false;
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
                if ac.fighter.strike_connects(pp, py, epos, &mut ec.fighter) {
                    landed = true;
                }
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
                        if ec.fighter.strike_connects(epos, ey, pp, &mut ac.fighter) {
                            landed = true;
                        }
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
        }
        if landed {
            // Strikes in this model always land on flesh
            // (no parry state), so they play the hurt
            // sound; the block variants stay reserved.
            if let Some(h) = &audio_handles {
                if let Some(hurt) = &h.hurt {
                    commands.spawn((AudioPlayer(hurt.clone()), PlaybackSettings::DESPAWN));
                    println!("[t2t] audio: sword hurt (event 0x17000f)");
                }
            }
        }
        // Player death -> reset the whole fight.
        if let Ok(mut ac) = q.single_mut() {
            if ac.fighter.state == FighterState::Dead {
                let rp = snap_respawn(&spec.soup, ac.checkpoint.respawn_point(spec.spawn));
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

        }
    }
}

/// Segmented HUD bars (health red, sand gold) + enemy HP
/// text + slow-mo readout. Split out of animate_char:
/// Bevy function systems cap at 16 params and that one
/// was already at 12. Prince HP/sand live in the bars;
/// the text keeps enemy HP + slow-mo.
fn hud_system(
    settings: Res<Settings>,
    spec: Option<Res<CharSpec>>,
    players: Query<&AnimatedChar>,
    foes: Query<&EnemyChar>,
    mut q_segs: Query<(&HudSeg, &mut BackgroundColor)>,
    mut q_hudvis: Query<&mut Visibility, Or<(With<HudRoot>, With<HudText>)>>,
    mut q_hud: Query<&mut Text, With<HudText>>,
) {
    let (Ok(ac), Some(spec)) = (players.single(), spec.as_deref()) else {
        return;
    };
    let hud_vis = if settings.interface_on {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut q_hudvis {
        *v = hud_vis;
    }
    for (seg, mut bg) in &mut q_segs {
        let filled = match seg.kind {
            HudSegKind::Hp => (seg.idx as i32) < ac.fighter.hp,
            HudSegKind::Sand => (seg.idx as u32) < ac.sand.sand,
        };
        let color = if filled {
            match seg.kind {
                HudSegKind::Hp => Color::srgb(0.85, 0.15, 0.15),
                HudSegKind::Sand => Color::srgb(0.9, 0.75, 0.3),
            }
        } else {
            Color::srgb(0.2, 0.2, 0.22)
        };
        *bg = BackgroundColor(color);
    }
    if let Ok(mut text) = q_hud.single_mut() {
        let mut hud = String::new();
        for ec in &foes {
            let e_spec = &spec.enemies[ec.idx];
            hud.push_str(&format!(
                "    {} HP {} / {}",
                e_spec.name, ec.fighter.hp, ec.fighter.max_hp
            ));
        }
        let slow = if ac.slowmo_t > 0.0 {
            format!("    [SLOW-MO {:.1}s]", ac.slowmo_t)
        } else {
            String::new()
        };
        **text = format!("{}{}", hud, slow);
    }
}

/// Build one Bevy mesh + material per world texture group: real
/// sampled textures replace the old per-element average colours.
/// Groups without a decoded texture fall back to flat grey.
/// A decoded RGBA texture as a mipmapped Bevy image: the CPU
/// box-filter chain from texture::mip_chain, uploaded as one
/// tightly-packed buffer (wgpu's create_texture_with_data
/// splits it per level). Image::new can't be used here —
/// it asserts data.len() == level-0 size — so the image is
/// built uninit and the descriptor count set by hand.
fn mipmapped_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    let (data, levels) = mip_chain(&rgba, width, height);
    let mut img = Image::new_uninit(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    img.texture_descriptor.mip_level_count = levels;
    img.data = Some(data);
    img
}

fn world_group_assets(
    w: &WorldMesh,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Vec<(Handle<Mesh>, Handle<StandardMaterial>)> {
    let mut tex_handles: std::collections::HashMap<u32, Handle<Image>> =
        std::collections::HashMap::new();
    for t in &w.textures {
        let mut img = mipmapped_image(t.width, t.height, t.rgba.clone());
        // Jade UVs tile past [0, 1] on walls and floors.
        // Linear mipmap filtering: without it the GPU
        // samples level 0 at every distance and distant
        // walls shimmer.
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mipmap_filter: ImageFilterMode::Linear,
            ..Default::default()
        });
        tex_handles.insert(t.key, images.add(img));
    }
    w.groups
        .iter()
        .map(|g| {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, g.positions.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, g.normals.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs.clone());
            mesh.insert_indices(Indices::U32(g.indices.clone()));
            let mat = match g.tex_key.and_then(|k| tex_handles.get(&k)) {
                Some(h) => {
                    let cutout = w
                        .textures
                        .iter()
                        .find(|t| Some(t.key) == g.tex_key)
                        .map_or(false, |t| t.cutout);
                    StandardMaterial {
                        base_color: Color::WHITE,
                        base_color_texture: Some(h.clone()),
                        alpha_mode: if cutout {
                            AlphaMode::Mask(0.5)
                        } else {
                            AlphaMode::Opaque
                        },
                        // M8: level shells are single-sided and
                        // face outward — from inside a room
                        // (Depot) the walls are backfaces and
                        // vanish under default culling. Render
                        // both sides; outdoors this changes
                        // nothing (front faces still win depth).
                        cull_mode: None,
                        ..default()
                    }
                }
                None => StandardMaterial {
                    base_color: Color::srgb(0.6, 0.58, 0.54),
                    cull_mode: None,
                    ..default()
                },
            };
            (meshes.add(mesh), materials.add(mat))
        })
        .collect()
}

/// Material for a character atlas. The sampler stays
/// ClampToEdge (an atlas must not repeat, or faces would
/// sample neighbouring stacked textures) with Linear
/// mipmap filtering added.
fn char_atlas_material(
    at: &CharAtlas,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Handle<StandardMaterial> {
    let mut img = mipmapped_image(at.width, at.height, at.rgba.clone());
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mipmap_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(images.add(img)),
        alpha_mode: if at.cutout {
            AlphaMode::Mask(0.5)
        } else {
            AlphaMode::Opaque
        },
        ..default()
    })
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    archive: Option<Res<GameArchive>>,
    world: Option<Res<WorldMesh>>,
    char_spec: Option<Res<CharSpec>>,
    phase: Option<Res<Phase>>,
    menu_world: Option<Res<MenuWorld>>,
    audio: Option<Res<GameAudio>>,
    mut audio_assets: ResMut<Assets<AudioSource>>,
    intro: Option<ResMut<IntroState>>,
    cine: Option<ResMut<CineState>>,
    mut images: ResMut<Assets<Image>>,
    settings: Res<Settings>,
) {
    // M7: in menu phase the gameplay scene starts hidden
    // behind the Menu3D backdrop; the transition flips
    // visibility (menu_update).
    let in_menu_start = matches!(phase.as_deref(), Some(Phase::Menu));
    let separate_menu = menu_world.as_deref().map_or(false, |m| m.0.is_some());
    let game_vis = if in_menu_start && separate_menu {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    let ui_vis = if in_menu_start {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if let Some(a) = archive {
        println!(
            "[t2t] engine ready — BIG v{}, {} files, {} dirs indexed",
            a.version, a.file_count, a.dir_count
        );
    }
    if let Some(w) = world {
        println!(
            "[t2t] world mesh: {} ({} tris, {} texture groups)",
            w.source_name,
            w.tris,
            w.groups.len()
        );
        for (mesh_h, mat_h) in world_group_assets(&w, &mut meshes, &mut materials, &mut images) {
            commands.spawn((
                Mesh3d(mesh_h),
                MeshMaterial3d(mat_h),
                Transform::default(),
                GameEntity,
                game_vis,
            ));
        }
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
        // Textured skins need no vertex tint; flat fallback
        // keeps the old beige.
        if spec.atlas.is_none() {
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_COLOR,
                vec![[0.85f32, 0.79, 0.7, 1.0]; n],
            );
        }
        mesh.insert_indices(Indices::U32((0..n as u32).collect()));
        let handle = meshes.add(mesh);
        let player_mat = match &spec.atlas {
            Some(at) => char_atlas_material(at, &mut materials, &mut images),
            None => materials.add(StandardMaterial {
                base_color: Color::WHITE,
                ..default()
            }),
        };
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
            MeshMaterial3d(player_mat),
            Transform::default(),
            GameEntity,
            game_vis,
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
            if e.atlas.is_none() {
                emesh.insert_attribute(
                    Mesh::ATTRIBUTE_COLOR,
                    vec![[0.72f32, 0.62, 0.5, 1.0]; en_n],
                );
            }
            emesh.insert_indices(Indices::U32((0..en_n as u32).collect()));
            let ehandle = meshes.add(emesh);
            let enemy_mat = match &e.atlas {
                Some(at) => char_atlas_material(at, &mut materials, &mut images),
                None => materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    ..default()
                }),
            };
            println!("[t2t] enemy: {} ({} corners)", e.name, en_n);
            commands.spawn((
                Mesh3d(ehandle.clone()),
                MeshMaterial3d(enemy_mat),
                Transform::default(),
                GameEntity,
                game_vis,
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
                    top: Val::Px(52.0),
                    left: Val::Px(14.0),
                    ..default()
                },
                HudText,
                GameEntity,
                game_vis,
            ));
        }
        // Segmented HUD bars: health (Prince max HP) and
        // sand tank, filled per frame by animate_char.
        let hp_n = player_fighter().max_hp as usize;
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(14.0),
                    left: Val::Px(14.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                HudRoot,
                GameEntity,
                game_vis,
            ))
            .with_children(|p| {
                p.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(3.0),
                    ..default()
                })
                .with_children(|r| {
                    for i in 0..hp_n {
                        r.spawn((
                            Node {
                                width: Val::Px(26.0),
                                height: Val::Px(14.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.2, 0.2, 0.22)),
                            HudSeg {
                                kind: HudSegKind::Hp,
                                idx: i,
                            },
                        ));
                    }
                });
                p.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(3.0),
                    ..default()
                })
                .with_children(|r| {
                    for i in 0..SAND_MAX as usize {
                        r.spawn((
                            Node {
                                width: Val::Px(26.0),
                                height: Val::Px(14.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.2, 0.2, 0.22)),
                            HudSeg {
                                kind: HudSegKind::Sand,
                                idx: i,
                            },
                        ));
                    }
                });
            });
    }
    // M7: the menu backdrop (the game's real Menu3D
    // scene) + title UI.
    if let Some(mw) = menu_world.as_deref() {
        if let Some(w) = &mw.0 {
            println!(
                "[t2t] menu backdrop: {} ({} tris, {} texture groups)",
                w.source_name,
                w.tris,
                w.groups.len()
            );
            for (mesh_h, mat_h) in world_group_assets(w, &mut meshes, &mut materials, &mut images) {
                commands.spawn((
                    Mesh3d(mesh_h),
                    MeshMaterial3d(mat_h),
                    Transform::default(),
                    MenuEntity,
                    ui_vis,
                ));
            }
        }
    }
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(16.0),
                left: Val::Percent(0.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                ..default()
            },
            MenuEntity,
            ui_vis,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("PRINCE OF PERSIA"),
                TextFont { font_size: 54.0, ..default() },
                TextColor(Color::srgb(0.92, 0.85, 0.68)),
            ));
            p.spawn((
                Text::new("T H E   T W O   T H R O N E S"),
                TextFont { font_size: 27.0, ..default() },
                TextColor(Color::srgb(0.75, 0.68, 0.55)),
            ));
            // Screen title / info line + the item list,
            // both filled by menu_list_system.
            p.spawn((
                Text::new(""),
                TextFont { font_size: 20.0, ..default() },
                TextColor(Color::srgb(0.85, 0.8, 0.7)),
                MenuInfo,
            ));
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                MenuList,
            ));
            p.spawn((
                Text::new("Arrows/W/S move \u{00b7} Enter select \u{00b7} Esc back"),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgb(0.6, 0.6, 0.62)),
            ));
            p.spawn((
                Text::new("WASD move \u{00b7} Space jump \u{00b7} F attack \u{00b7} R sand slow-motion"),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgb(0.6, 0.6, 0.62)),
            ));
        });
    // Pause overlay: dark panel over the frozen game,
    // filled by menu_list_system while Paused.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(0.0),
                top: Val::Percent(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            PauseUi,
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Paused"),
                TextFont { font_size: 40.0, ..default() },
                TextColor(Color::srgb(0.92, 0.85, 0.68)),
            ));
            p.spawn((
                Text::new(""),
                TextFont { font_size: 20.0, ..default() },
                TextColor(Color::srgb(0.85, 0.8, 0.7)),
                PauseInfo,
            ));
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                PauseList,
            ));
        });
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
    // M7 audio: register the decoded sounds; in the
    // menu phase start the looping music + boot sound
    // (deferred to the intro handover when the boot
    // intro is active).
    let intro_active = intro.as_ref().map_or(false, |i| i.active);
    if let Some(a) = audio {
        let handles = AudioHandles {
            music: audio_assets.add(AudioSource { bytes: a.music_wav.clone().into() }),
            ui_boot: audio_assets.add(AudioSource { bytes: a.ui_boot_wav.clone().into() }),
            ui_confirm: audio_assets.add(AudioSource {
                bytes: a.ui_confirm_wav.clone().into(),
            }),
            block: a
                .block_wavs
                .iter()
                .map(|w| audio_assets.add(AudioSource { bytes: w.clone().into() }))
                .collect(),
            hurt: a
                .hurt_wav
                .as_ref()
                .map(|w| audio_assets.add(AudioSource { bytes: w.clone().into() })),
        };
        if in_menu_start && !intro_active {
            if settings.music_on {
                commands.spawn((
                    AudioPlayer(handles.music.clone()),
                    PlaybackSettings {
                        volume: bevy::audio::Volume::Decibels(-6.0),
                        ..PlaybackSettings::LOOP
                    },
                    MusicPlayer,
                ));
            }
            commands.spawn((AudioPlayer(handles.ui_boot.clone()), PlaybackSettings::ONCE));
            println!("[t2t] audio: menu music + boot sound playing (event 0x190002)");
        }
        commands.insert_resource(handles);
    }
    // M7 intro: fullscreen video node + clip audio.
    if let Some(mut st) = intro {
        if st.active {
            let img = images.add(Image::new_fill(
                bevy::render::render_resource::Extent3d {
                    width: 800,
                    height: 600,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                &[0, 0, 0, 255],
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            ));
            let mut ch = Vec::new();
            for c in &st.clips {
                ch.push(
                    c.audio_wav
                        .as_ref()
                        .map(|w| audio_assets.add(AudioSource { bytes: w.clone().into() })),
                );
            }
            st.clip_audio = ch;
            st.img = Some(img.clone());
            commands.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ImageNode::new(img),
                MenuEntity,
                IntroNode,
                GlobalZIndex(10),
            ));
            println!("[t2t] intro: playing UBISOFT (Enter skips)");
        }
    }
    // Cine: fullscreen video node (hidden until triggered)
    // + clip audio. Deliberately NOT a MenuEntity: menu
    // transitions must never show or hide this node.
    if let Some(mut st) = cine {
        if !st.clips.is_empty() {
            let img = images.add(Image::new_fill(
                bevy::render::render_resource::Extent3d {
                    width: 640,
                    height: 448,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                &[0, 0, 0, 255],
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            ));
            let mut ch = Vec::new();
            for c in &st.clips {
                ch.push(
                    c.audio_wav
                        .as_ref()
                        .map(|w| audio_assets.add(AudioSource { bytes: w.clone().into() })),
                );
            }
            st.clip_audio = ch;
            st.img = Some(img.clone());
            commands.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ImageNode::new(img),
                CineNode,
                GlobalZIndex(11),
                Visibility::Hidden,
            ));
        }
    }
}

/// Boot intro playback: advance the current clip at
/// 29.97 fps, uploading each new JPEG frame into the
/// shared image; play the clip's audio at its start.
/// Enter / Space / click skips the clip. After the
/// last clip, hide the node and start the menu audio
/// (the handover setup deferred).
#[allow(clippy::too_many_arguments)]
fn intro_update(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    intro: Option<ResMut<IntroState>>,
    mut images: ResMut<Assets<Image>>,
    audio_handles: Option<Res<AudioHandles>>,
    mut node_q: Query<&mut Visibility, With<IntroNode>>,
    music_q: Query<Entity, With<MusicPlayer>>,
) {
    let Some(mut intro) = intro else {
        return;
    };
    if !intro.active {
        return;
    }
    intro.t += time.delta_secs();
    let skip = keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || mouse.just_pressed(MouseButton::Left);
    let clip_len = intro.clips[intro.clip].frames.len();
    let frame = (intro.t * 29.97) as usize;
    if skip || frame >= clip_len {
        intro.clip += 1;
        intro.t = 0.0;
        intro.shown = usize::MAX;
        intro.started = false;
        if intro.clip >= intro.clips.len() {
            intro.active = false;
            for mut v in &mut node_q {
                *v = Visibility::Hidden;
            }
            if let Some(h) = audio_handles {
                // A video over the menu may already have
                // restarted the music on its own return;
                // never run two menu-music players.
                if music_q.is_empty() {
                    commands.spawn((
                        AudioPlayer(h.music.clone()),
                        PlaybackSettings {
                            volume: bevy::audio::Volume::Decibels(-6.0),
                            ..PlaybackSettings::LOOP
                        },
                        MusicPlayer,
                    ));
                }
                commands.spawn((AudioPlayer(h.ui_boot.clone()), PlaybackSettings::ONCE));
                println!("[t2t] intro done -> menu (music + boot sound)");
            }
            return;
        }
        println!("[t2t] intro: {}", intro.clips[intro.clip].name);
        return;
    }
    if !intro.started {
        intro.started = true;
        if let Some(Some(h)) = intro.clip_audio.get(intro.clip) {
            let h = h.clone();
            commands.spawn((AudioPlayer(h), PlaybackSettings::DESPAWN));
        }
    }
    if frame != intro.shown {
        intro.shown = frame;
        let jpeg = intro.clips[intro.clip].frames.get(frame).cloned();
        if let (Some(jpeg), Some(handle)) = (jpeg, intro.img.clone()) {
            if let Ok(dynimg) = image::load_from_memory(&jpeg) {
                let rgba = dynimg.to_rgba8();
                if rgba.width() == 800 && rgba.height() == 600 {
                    if let Some(img) = images.get_mut(&handle) {
                        img.data = Some(rgba.into_raw());
                    }
                }
            }
        }
    }
}

/// Cine trigger: boot activation (no T2T_CINE_AT) fires on
/// the first frame, returning to the boot phase; a staged
/// T2T_CINE_AT fires once after that many seconds of
/// Playing time, returning to Playing. Activation shows
/// the Cine node and freezes gameplay (all gameplay
/// systems are gated on Phase::Playing).
fn cine_trigger(
    mut commands: Commands,
    cine: Option<ResMut<CineState>>,
    mut phase: ResMut<Phase>,
    time: Res<Time>,
    mut node_q: Query<&mut Visibility, With<CineNode>>,
    mut music_q: Query<Entity, With<MusicPlayer>>,
    credits_req: Option<Res<CreditsRequest>>,
) {
    let Some(mut cine) = cine else {
        return;
    };
    if cine.active {
        return;
    }
    // Credits request wins over the staged triggers and
    // plays just the credits clip (stop_after).
    let mut fire_clip: Option<usize> = None;
    let mut fire_stop: Option<usize> = None;
    if credits_req.is_some() {
        commands.remove_resource::<CreditsRequest>();
        if let Some(idx) = cine.credits_idx {
            fire_clip = Some(idx);
            fire_stop = Some(idx + 1);
        } else {
            println!("[t2t] cine: credits requested but no credits clip in cache");
            return;
        }
    } else if cine.pending {
        cine.pending = false;
        fire_clip = Some(0);
    } else if *phase == Phase::Playing {
        if let Some(at) = cine.at_secs {
            cine.elapsed += time.delta_secs();
            if cine.elapsed >= at {
                cine.at_secs = None;
                fire_clip = Some(0);
            }
        }
    }
    if let Some(c) = fire_clip {
        cine.return_to = *phase;
        // WITHOUT this line the phase never becomes Cine, so
        // cine_update (run_if in_cine) never runs: the node
        // sits black over a still-running game forever.
        *phase = Phase::Cine;
        cine.active = true;
        cine.clip = c;
        cine.stop_after = fire_stop;
        cine.t = 0.0;
        cine.shown = usize::MAX;
        cine.started = false;
        // A video replaces the menu music; cine_update
        // restarts it on return to Menu.
        if !music_q.is_empty() {
            for e in music_q.iter() {
                commands.entity(e).despawn();
            }
            println!("[t2t] audio: music stopped for video");
        }
        for mut v in &mut node_q {
            *v = Visibility::Visible;
        }
        println!(
            "[t2t] cine: playing {} (return to {:?})",
            cine.clips[cine.clip].name, cine.return_to
        );
    }
}

/// Cine playback: same frame-streaming as the intro, at
/// 29.97 fps, into the Cine node's image. Enter / Space /
/// click skips the current clip. On end, the node hides,
/// the clip audio stops, and the phase returns to
/// `return_to` (gameplay resumes where it froze).
#[allow(clippy::too_many_arguments)]
fn cine_update(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cine: Option<ResMut<CineState>>,
    mut images: ResMut<Assets<Image>>,
    mut phase: ResMut<Phase>,
    mut node_q: Query<(&mut Visibility, &mut ImageNode), With<CineNode>>,
    settings: Res<Settings>,
    audio_handles: Option<Res<AudioHandles>>,
    music_q: Query<Entity, With<MusicPlayer>>,
) {
    let Some(mut cine) = cine else {
        return;
    };
    if !cine.active {
        return;
    }
    cine.t += time.delta_secs();
    let skip = keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || mouse.just_pressed(MouseButton::Left);
    let clip_len = cine.clips[cine.clip].frames.len();
    let frame = (cine.t * 29.97) as usize;
    if skip || frame >= clip_len {
        if let Some(e) = cine.audio_entity.take() {
            commands.entity(e).despawn();
        }
        cine.clip += 1;
        cine.t = 0.0;
        cine.shown = usize::MAX;
        cine.started = false;
        if cine.clip >= cine.clips.len() || Some(cine.clip) == cine.stop_after {
            cine.active = false;
            cine.stop_after = None;
            for (mut v, _) in &mut node_q {
                *v = Visibility::Hidden;
            }
            let back = cine.return_to;
            *phase = back;
            if back == Phase::Menu && settings.music_on && music_q.is_empty() {
                if let Some(h) = audio_handles {
                    commands.spawn((
                        AudioPlayer(h.music.clone()),
                        PlaybackSettings {
                            volume: bevy::audio::Volume::Decibels(-6.0),
                            ..PlaybackSettings::LOOP
                        },
                        MusicPlayer,
                    ));
                    println!("[t2t] audio: menu music restarted after video");
                }
            }
            println!("[t2t] cine done -> {:?}", back);
            return;
        }
        println!("[t2t] cine: {}", cine.clips[cine.clip].name);
        return;
    }
    if !cine.started {
        cine.started = true;
        if let Some(Some(h)) = cine.clip_audio.get(cine.clip) {
            let h = h.clone();
            let e = commands.spawn((AudioPlayer(h), PlaybackSettings::DESPAWN)).id();
            cine.audio_entity = Some(e);
        }
    }
    if frame != cine.shown {
        cine.shown = frame;
        let jpeg = cine.clips[cine.clip].frames.get(frame).cloned();
        if let (Some(jpeg), Some(handle)) = (jpeg, cine.img.clone()) {
            if let Ok(dynimg) = image::load_from_memory(&jpeg) {
                let rgba = dynimg.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let cur = images
                    .get(&handle)
                    .map(|im| (im.texture_descriptor.size.width, im.texture_descriptor.size.height));
                if cur == Some((w, h)) {
                    if let Some(img) = images.get_mut(&handle) {
                        img.data = Some(rgba.into_raw());
                    }
                } else {
                    // First frame of a differently-sized clip
                    // (credits is 800x600, Cine clips are
                    // 640x448): swap the node image in.
                    let new_img = images.add(Image::new(
                        bevy::render::render_resource::Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        bevy::render::render_resource::TextureDimension::D2,
                        rgba.into_raw(),
                        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                        RenderAssetUsages::default(),
                    ));
                    for (_, mut node_img) in &mut node_q {
                        node_img.image = new_img.clone();
                    }
                    cine.img = Some(new_img);
                }
            }
        }
    }
}

/// Menu phase: slowly orbit the Menu3D backdrop and
/// watch for the start signal (Enter, or the staged
/// T2T_MENU_SECS timer). On start, hide the menu
/// presentation, reveal the gameplay scene, and hand
/// the camera to animate_char.
fn menu_update(
    time: Res<Time>,
    mut state: ResMut<MenuState>,
    mut cam: Query<&mut Transform, With<FollowCam>>,
) {
    state.t += time.delta_secs();
    // Menu3D is a skybox ring (~40 across) around a
    // small central island — the camera must orbit
    // INSIDE the ring, close to the island, or the
    // near panorama panel occludes everything. Framing
    // from menu_probe: the island mass sits at Bevy
    // y ~ -4.8 (centroid x -0.5, z 1.0), structures
    // rising to ~-1; orbit just above the island plane.
    let a = state.t * 0.07;
    for mut t in &mut cam {
        *t = Transform::from_xyz(
            -0.5 + a.cos() * 10.0,
            -2.2 + (state.t * 0.23).sin() * 0.6,
            1.0 + a.sin() * 10.0,
        )
        .looking_at(Vec3::new(-0.5, -3.0, 1.0), Vec3::Y);
    }
}

/// Reset a finished (or fresh) session to its spawn state:
/// the player back at the spec spawn with full HP and sand,
/// every enemy home with full HP. Runs on every Start New
/// Game — on a first start it is a no-op over fresh state.
fn reset_session(spec: &CharSpec, players: &mut Query<&mut AnimatedChar>, foes: &mut Query<&mut EnemyChar>) {
    for mut ac in players.iter_mut() {
        ac.ctl = Controller::new(spec.spawn);
        ac.fighter = player_fighter();
        ac.sand = SandTank::new(SAND_MAX, start_sand());
        ac.slowmo_t = 0.0;
        ac.elapsed = 0.0;
        ac.slowmo_staged = false;
        ac.time = 0.0;
        ac.air_time = 0.0;
        ac.hang_time = 0.0;
        ac.pole_time = 0.0;
        ac.checkpoint = seed_checkpoint();
        ac.orbit_yaw = 0.0;
        ac.orbit_pitch = 0.25;
    }
    for mut ec in foes.iter_mut() {
        ec.ctl = Controller::new(ec.home);
        ec.fighter = enemy_fighter();
        ec.dead_time = 0.0;
        ec.sand_paid = false;
        ec.time = 0.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_menu_action(
    action: MenuAction,
    commands: &mut Commands,
    phase: &mut Phase,
    state: &mut MenuState,
    settings: &mut Settings,
    audio_handles: Option<&AudioHandles>,
    music_q: &mut Query<Entity, With<MusicPlayer>>,
    menu_q: &mut Query<&mut Visibility, (With<MenuEntity>, Without<GameEntity>)>,
    game_q: &mut Query<&mut Visibility, (With<GameEntity>, Without<MenuEntity>)>,
    separate_menu: bool,
    spec: Option<&CharSpec>,
    players: &mut Query<&mut AnimatedChar>,
    foes: &mut Query<&mut EnemyChar>,
    exit: &mut EventWriter<AppExit>,
) {
    match action {
        MenuAction::StartGame => {
            if !state.has_char {
                println!("[t2t] menu: no playable sector loaded (start ignored)");
                state.auto_secs = None;
                return;
            }
            println!("[t2t] menu -> playing");
            if let Some(spec) = spec {
                reset_session(spec, players, foes);
            }
            if let Some(h) = audio_handles {
                for e in music_q.iter() {
                    commands.entity(e).despawn();
                }
                commands.spawn((AudioPlayer(h.ui_confirm.clone()), PlaybackSettings::ONCE));
                println!("[t2t] audio: music stopped, confirm sound (event 0x190007)");
            }
            for mut v in menu_q.iter_mut() {
                *v = Visibility::Hidden;
            }
            for mut v in game_q.iter_mut() {
                *v = Visibility::Visible;
            }
            state.stack.clear();
            *phase = Phase::Playing;
        }
        MenuAction::ToScreen(screen) => {
            state.stack.push(state.screen);
            state.screen = screen;
            state.selected = 0;
        }
        MenuAction::Back => {
            if let Some(prev) = state.stack.pop() {
                state.screen = prev;
            } else if state.screen == MenuScreen::Pause {
                state.stack.clear();
                *phase = Phase::Playing;
            } else {
                state.screen = MenuScreen::Main;
            }
            state.selected = 0;
        }
        MenuAction::Resume => {
            state.stack.clear();
            *phase = Phase::Playing;
        }
        MenuAction::QuitToMenu => {
            println!("[t2t] game -> menu");
            if separate_menu {
                for mut v in game_q.iter_mut() {
                    *v = Visibility::Hidden;
                }
            }
            for mut v in menu_q.iter_mut() {
                *v = Visibility::Inherited;
            }
            if settings.music_on {
                if let Some(h) = audio_handles {
                    commands.spawn((AudioPlayer(h.music.clone()), PlaybackSettings::LOOP));
                    println!("[t2t] audio: menu music restarted");
                }
            }
            state.screen = MenuScreen::Main;
            state.stack.clear();
            state.selected = 0;
            *phase = Phase::Menu;
        }
        MenuAction::QuitApp => {
            println!("[t2t] menu: quit confirmed");
            exit.write(AppExit::Success);
        }
        MenuAction::ToggleMusic => {
            settings.music_on = !settings.music_on;
            if settings.music_on {
                if let Some(h) = audio_handles {
                    commands.spawn((AudioPlayer(h.music.clone()), PlaybackSettings::LOOP));
                }
                println!("[t2t] settings: music on");
            } else {
                for e in music_q.iter() {
                    commands.entity(e).despawn();
                }
                println!("[t2t] settings: music off");
            }
        }
        MenuAction::ToggleInterface => {
            settings.interface_on = !settings.interface_on;
            println!("[t2t] settings: interface {}", settings.interface_on);
        }
        MenuAction::PlayCredits => {
            // The Cine player picks this up next frame
            // (and reports a missing credits clip itself).
            commands.insert_resource(CreditsRequest);
            println!("[t2t] menu: credits requested");
        }
        MenuAction::Noop => {}
    }
}

fn menu_navigate(state: &mut MenuState, len: usize, down: bool, up: bool) {
    if len == 0 {
        return;
    }
    if down {
        state.selected = (state.selected + 1) % len;
    }
    if up {
        state.selected = (state.selected + len - 1) % len;
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_nav(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut phase: ResMut<Phase>,
    mut state: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    strings: Res<MenuStrings>,
    audio_handles: Option<Res<AudioHandles>>,
    mut music_q: Query<Entity, With<MusicPlayer>>,
    mut menu_q: Query<&mut Visibility, (With<MenuEntity>, Without<GameEntity>)>,
    mut game_q: Query<&mut Visibility, (With<GameEntity>, Without<MenuEntity>)>,
    menu_world: Option<Res<MenuWorld>>,
    spec: Option<Res<CharSpec>>,
    mut players: Query<&mut AnimatedChar>,
    mut foes: Query<&mut EnemyChar>,
    mut exit: EventWriter<AppExit>,
    intro: Option<Res<IntroState>>,
) {
    // Auto-start (staging) bypasses navigation.
    if let Some(s) = state.auto_secs {
        if state.t >= s {
            state.auto_secs = None;
            let separate = menu_world
                .as_deref()
                .map_or(false, |m| m.0.is_some());
            apply_menu_action(
                MenuAction::StartGame,
                &mut commands,
                &mut phase,
                &mut state,
                &mut settings,
                audio_handles.as_deref(),
                &mut music_q,
                &mut menu_q,
                &mut game_q,
                separate,
                spec.as_deref(),
                &mut players,
                &mut foes,
                &mut exit,
            );
            return;
        }
    }
    // While the boot intro plays, Enter/Space belong to
    // it (skip), not to menu navigation.
    if intro.as_ref().map_or(false, |i| i.active) {
        return;
    }
    let items = menu_items(state.screen, &strings, &settings);
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) {
        menu_navigate(&mut state, items.len(), true, false);
    }
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
        menu_navigate(&mut state, items.len(), false, true);
    }
    if keys.just_pressed(KeyCode::Enter) {
        if let Some(item) = items.get(state.selected) {
            let action = item.action;
            let separate = menu_world
                .as_deref()
                .map_or(false, |m| m.0.is_some());
            apply_menu_action(
                action,
                &mut commands,
                &mut phase,
                &mut state,
                &mut settings,
                audio_handles.as_deref(),
                &mut music_q,
                &mut menu_q,
                &mut game_q,
                separate,
                spec.as_deref(),
                &mut players,
                &mut foes,
                &mut exit,
            );
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        let action = if state.screen == MenuScreen::Main {
            MenuAction::ToScreen(MenuScreen::ConfirmQuit)
        } else {
            MenuAction::Back
        };
        let separate = menu_world
            .as_deref()
            .map_or(false, |m| m.0.is_some());
        apply_menu_action(
            action,
            &mut commands,
            &mut phase,
            &mut state,
            &mut settings,
            audio_handles.as_deref(),
            &mut music_q,
            &mut menu_q,
            &mut game_q,
            separate,
            spec.as_deref(),
            &mut players,
            &mut foes,
            &mut exit,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn pause_nav(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut phase: ResMut<Phase>,
    mut state: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    strings: Res<MenuStrings>,
    audio_handles: Option<Res<AudioHandles>>,
    mut music_q: Query<Entity, With<MusicPlayer>>,
    mut menu_q: Query<&mut Visibility, (With<MenuEntity>, Without<GameEntity>)>,
    mut game_q: Query<&mut Visibility, (With<GameEntity>, Without<MenuEntity>)>,
    menu_world: Option<Res<MenuWorld>>,
    spec: Option<Res<CharSpec>>,
    mut players: Query<&mut AnimatedChar>,
    mut foes: Query<&mut EnemyChar>,
    mut exit: EventWriter<AppExit>,
) {
    let items = menu_items(state.screen, &strings, &settings);
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) {
        menu_navigate(&mut state, items.len(), true, false);
    }
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
        menu_navigate(&mut state, items.len(), false, true);
    }
    if keys.just_pressed(KeyCode::Enter) {
        if let Some(item) = items.get(state.selected) {
            let action = item.action;
            let separate = menu_world
                .as_deref()
                .map_or(false, |m| m.0.is_some());
            apply_menu_action(
                action,
                &mut commands,
                &mut phase,
                &mut state,
                &mut settings,
                audio_handles.as_deref(),
                &mut music_q,
                &mut menu_q,
                &mut game_q,
                separate,
                spec.as_deref(),
                &mut players,
                &mut foes,
                &mut exit,
            );
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        let action = if state.screen == MenuScreen::Pause {
            MenuAction::Resume
        } else {
            MenuAction::Back
        };
        let separate = menu_world
            .as_deref()
            .map_or(false, |m| m.0.is_some());
        apply_menu_action(
            action,
            &mut commands,
            &mut phase,
            &mut state,
            &mut settings,
            audio_handles.as_deref(),
            &mut music_q,
            &mut menu_q,
            &mut game_q,
            separate,
            spec.as_deref(),
            &mut players,
            &mut foes,
            &mut exit,
        );
    }
}

fn pause_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mut phase: ResMut<Phase>,
    mut state: ResMut<MenuState>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        state.screen = MenuScreen::Pause;
        state.stack.clear();
        state.selected = 0;
        *phase = Phase::Paused;
        println!("[t2t] game paused");
    }
}

fn pause_ui_visibility(
    phase: Res<Phase>,
    mut q: Query<&mut Visibility, With<PauseUi>>,
) {
    let vis = if *phase == Phase::Paused {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in &mut q {
        *v = vis;
    }
}

/// Rebuild the menu (or pause) item list whenever the
/// screen, selection, or a toggle value changes.
fn menu_list_system(
    mut commands: Commands,
    phase: Res<Phase>,
    mut state: ResMut<MenuState>,
    strings: Res<MenuStrings>,
    settings: Res<Settings>,
    menu_lists: Query<Entity, With<MenuList>>,
    pause_lists: Query<Entity, With<PauseList>>,
    all_children: Query<&Children>,
    // One query for both info nodes (menu + pause): two
    // &mut Text queries conflict at runtime (B0001). Both
    // nodes get the phase-appropriate string; the hidden
    // one is corrected on the frame its phase opens.
    mut info_q: Query<&mut Text, Or<(With<MenuInfo>, With<PauseInfo>)>>,
) {
    let key = (
        state.screen,
        state.selected,
        settings.music_on,
        settings.interface_on,
    );
    let in_menu = *phase == Phase::Menu;
    let built = if in_menu {
        &mut state.built
    } else {
        &mut state.pause_built
    };
    if *built == Some(key) {
        return;
    }
    *built = Some(key);
    let items = menu_items(state.screen, &strings, &settings);
    let (title, info) = menu_screen_text(state.screen, &strings);
    // NOTE: a childless container has NO Children component,
    // so the list entity must be queried without &Children
    // (the first build found nothing and the menu stayed
    // empty — the old query never matched).
    let list_e = if in_menu {
        menu_lists.single().ok()
    } else {
        pause_lists.single().ok()
    };
    let list_data: Option<(Entity, Vec<Entity>)> = list_e.map(|e| {
        let children = all_children
            .get(e)
            .map(|ch| ch.iter().collect())
            .unwrap_or_default();
        (e, children)
    });
    let info_value = if in_menu {
        if title.is_empty() {
            info.clone()
        } else if info.is_empty() {
            title.clone()
        } else {
            format!("{title}\n{info}")
        }
    } else if state.screen == MenuScreen::Pause {
        // The pause root already titles itself "Paused".
        String::new()
    } else if info.is_empty() {
        title.clone()
    } else {
        format!("{title}\n{info}")
    };
    for mut text in &mut info_q {
        **text = info_value.clone();
    }
    if let Some((list_e, children)) = list_data {
        for child in children {
            commands.entity(child).despawn();
        }
        let selected = state.selected;
        commands.entity(list_e).with_children(|p| {
            for (i, item) in items.iter().enumerate() {
                p.spawn((
                    Text::new(item.label.clone()),
                    TextFont {
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(if i == selected {
                        Color::srgb(0.92, 0.85, 0.68)
                    } else {
                        Color::srgb(0.72, 0.72, 0.76)
                    }),
                ));
            }
        });
    }
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
        app.insert_resource(Portals(load_portals(&bf_path, &wow_name)));
        if let Some(spec) = prepare_character(&bf_path, &wow_name, &world) {
            app.insert_resource(spec);
            have_char = true;
        }
        app.insert_resource(world);
    } else {
        eprintln!("[t2t] no parseable geometry found in wow matching '{wow_name}'");
    }

    // M7: boot into the menu phase unless staging says
    // otherwise; the Menu3D world is the backdrop (when
    // the game sector IS Menu3D it doubles as both).
    let start_playing = std::env::var("T2T_START").map_or(false, |v| v == "play");
    // Staging: T2T_START_PAUSED=1 boots into gameplay
    // with the pause overlay open (verifies the panel
    // without key injection).
    let start_paused = std::env::var("T2T_START_PAUSED").map_or(false, |v| v == "1");
    app.insert_resource(if start_playing {
        Phase::Playing
    } else if start_paused {
        Phase::Paused
    } else {
        Phase::Menu
    });
    app.insert_resource(load_menu_strings(&bf_path));
    app.insert_resource(Settings::default());
    let menu_world = if wow_name.contains("Menu3D") {
        None
    } else {
        prepare_world(&bf_path, "Menu3D")
    };
    app.insert_resource(MenuWorld(menu_world));
    // Staging: T2T_MENU_SCREEN=<name> boots the menu on
    // that screen (main/options/graphics/sounds/controls/
    // game/profiles/load/credits/confirm).
    let boot_screen = if start_paused {
        MenuScreen::Pause
    } else {
        match std::env::var("T2T_MENU_SCREEN").as_deref() {
            Ok("options") => MenuScreen::Options,
            Ok("graphics") => MenuScreen::Graphics,
            Ok("sounds") => MenuScreen::Sounds,
            Ok("controls") => MenuScreen::Controls,
            Ok("game") => MenuScreen::GameOpts,
            Ok("profiles") => MenuScreen::Profiles,
            Ok("load") => MenuScreen::LoadGame,
            Ok("credits") => MenuScreen::Credits,
            Ok("confirm") => MenuScreen::ConfirmQuit,
            _ => MenuScreen::Main,
        }
    };
    app.insert_resource(MenuState {
        t: 0.0,
        auto_secs: std::env::var("T2T_MENU_SECS")
            .ok()
            .and_then(|v| v.parse::<f32>().ok()),
        has_char: have_char,
        screen: boot_screen,
        stack: Vec::new(),
        selected: 0,
        built: None,
        pause_built: None,
    });
    // M7 audio (optional: absence only silences the game).
    match prepare_audio(&bf_path) {
        Some(ga) => {
            app.insert_resource(ga);
        }
        None => eprintln!("[t2t] audio unavailable (Sound/ data not decoded) — continuing silent"),
    }
    // M7 boot intro (optional: absence boots to menu).
    // Booting paused also skips the intro: its overlay
    // would otherwise cover the screen while no system
    // advances it (intro_update only runs in Menu).
    if let Some(st) = prepare_intro(start_playing || start_paused) {
        app.insert_resource(st);
    }
    // Cine story clips + credits clip (optional:
    // absence runs with no Cine).
    if let Some(st) = prepare_cine() {
        app.insert_resource(st);
    }
    // Staging: play the credits clip at boot (the menu
    // item requests it the same way at runtime).
    if std::env::var_os("T2T_PLAY_CREDITS").is_some() {
        app.insert_resource(CreditsRequest);
    }

    app.add_systems(Startup, setup);
    app.add_systems(Update, intro_update.run_if(in_menu));
    app.add_systems(Update, cine_trigger);
    app.add_systems(Update, cine_update.run_if(in_cine));
    app.add_systems(Update, menu_update.run_if(in_menu));
    app.add_systems(Update, menu_nav.run_if(in_menu));
    app.add_systems(Update, menu_list_system.run_if(in_menu_or_paused));
    app.add_systems(Update, pause_toggle.run_if(in_playing));
    app.add_systems(Update, pause_nav.run_if(in_paused));
    app.add_systems(Update, pause_ui_visibility);
    app.add_systems(Update, hud_system.run_if(in_playing));
    if have_char {
        app.add_systems(Update, animate_char.run_if(in_playing));
        app.add_systems(Update, portal_watch.run_if(in_playing));
    }
    app.run();
}
