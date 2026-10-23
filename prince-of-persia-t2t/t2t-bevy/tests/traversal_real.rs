//! Traversal (M6.3): ledge grab, shimmy, climb-up, drop —
//! verified on a synthetic platform and scanned on the real
//! 100_Quai geometry.

use t2t::bigfile::BigFile;
use t2t::collision::TriangleSoup;
use t2t::controller::{Controller, MoveInput, HANG_DROP};
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;
use t2t::assemble::assemble;

const BF: &str = "../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf";

/// Platform: floor quad x[-200,200], y[0,400] at z=100,
/// open air at y<0, a wall face at y=0 (z 0..100).
fn platform_soup() -> TriangleSoup {
    let floor = [
        [-200.0, 0.0, 100.0], [200.0, 0.0, 100.0], [200.0, 400.0, 100.0],
        [-200.0, 0.0, 100.0], [200.0, 400.0, 100.0], [-200.0, 400.0, 100.0],
    ];
    let wall = [
        [-200.0, 0.0, 0.0], [200.0, 0.0, 100.0], [200.0, 0.0, 0.0],
        [-200.0, 0.0, 0.0], [-200.0, 0.0, 100.0], [200.0, 0.0, 100.0],
    ];
    let mut tris = Vec::new();
    for t in [floor, wall] {
        tris.push([
            t[0][0], t[0][1], t[0][2], t[1][0], t[1][1], t[1][2], t[2][0], t[2][1], t[2][2],
        ]);
        tris.push([
            t[3][0], t[3][1], t[3][2], t[4][0], t[4][1], t[4][2], t[5][0], t[5][1], t[5][2],
        ]);
    }
    TriangleSoup { tris }
}

fn idle() -> MoveInput {
    MoveInput { vel_xy: [0.0, 0.0], jump: false, drop: false }
}

#[test]
fn ledge_grab_shimmy_climb_drop() {
    let soup = platform_soup();
    // Airborne beside the wall, moving toward it (+Y).
    let mut ctl = Controller::new([0.0, -70.0, -60.0]);
    let dt = 1.0 / 60.0;
    let toward = MoveInput { vel_xy: [0.0, 150.0], jump: false, drop: false };
    let mut grabbed = false;
    for _ in 0..30 {
        ctl.step(&soup, &toward, dt);
        if ctl.hanging.is_some() {
            grabbed = true;
            break;
        }
    }
    assert!(grabbed, "should grab the platform edge");
    assert!((ctl.pos[2] - (100.0 - HANG_DROP)).abs() < 2.0, "hang height: {}", ctl.pos[2]);
    assert!(ctl.pos[1] < 0.0 && ctl.pos[1] > -30.0, "hangs just off the face: {}", ctl.pos[1]);

    // Shimmy +X along the wall.
    let x0 = ctl.pos[0];
    let shimmy = MoveInput { vel_xy: [150.0, 0.0], jump: false, drop: false };
    for _ in 0..30 {
        ctl.step(&soup, &shimmy, dt);
    }
    assert!(ctl.hanging.is_some(), "still hanging after shimmy");
    assert!(ctl.pos[0] > x0 + 40.0, "shimmy moved along the wall: {} -> {}", x0, ctl.pos[0]);
    assert!((ctl.pos[2] - (100.0 - HANG_DROP)).abs() < 2.0, "hang height kept");

    // Climb up: ends standing on the platform.
    let climb = MoveInput { vel_xy: [0.0, 0.0], jump: true, drop: false };
    ctl.step(&soup, &climb, dt);
    assert!(ctl.hanging.map(|h| h.climbing > 0.0).unwrap_or(false), "climb started");
    for _ in 0..40 {
        ctl.step(&soup, &idle(), dt);
    }
    assert!(ctl.hanging.is_none(), "climb finished");
    assert!(ctl.grounded, "standing after climb");
    assert!((ctl.pos[2] - 100.0).abs() < 2.0, "on the platform top: {}", ctl.pos[2]);
    assert!(ctl.pos[1] > 5.0, "landed inside the edge: {}", ctl.pos[1]);

    // Grab again, then drop: falls into the open air.
    let mut ctl2 = Controller::new([50.0, -70.0, -60.0]);
    for _ in 0..30 {
        ctl2.step(&soup, &toward, dt);
        if ctl2.hanging.is_some() {
            break;
        }
    }
    assert!(ctl2.hanging.is_some());
    let drop = MoveInput { vel_xy: [0.0, 0.0], jump: false, drop: true };
    ctl2.step(&soup, &drop, dt);
    assert!(ctl2.hanging.is_none(), "released");
    let z0 = ctl2.pos[2];
    for _ in 0..20 {
        ctl2.step(&soup, &idle(), dt);
    }
    assert!(!ctl2.grounded && ctl2.pos[2] < z0 - 20.0, "falling after drop");
}

#[test]
fn quai_has_grabbable_ledges() {
    let bf = BigFile::open(BF).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("100_Quai"))
        .expect("quai entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let soup = TriangleSoup::from_world(&world);
    let (mn, mx) = world.aabb();
    // Coarse scan: grid over the map, a few hand heights,
    // four facing directions.
    let mut found = 0usize;
    let mut x = mn[0] + 100.0;
    while x < mx[0] - 100.0 {
        let mut y = mn[1] + 100.0;
        while y < mx[1] - 100.0 {
            for &dir in &[[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]] {
                for &hz in &[60.0f32, 120.0, 250.0, 450.0, 800.0, 1400.0, 2200.0, 3200.0, 4500.0] {
                    if soup.find_ledge([x, y, hz - 170.0], dir, hz).is_some() {
                        found += 1;
                    }
                }
            }
            y += 200.0;
        }
        x += 200.0;
    }
    assert!(found > 0, "Quai should have grabbable ledges");
    println!("quai ledge hits: {found}");
}


/// Beam: floor strip x[-30,30], y[-400,400] at z=100.
fn beam_soup() -> TriangleSoup {
    let floor = [
        [-30.0, -400.0, 100.0], [30.0, -400.0, 100.0], [30.0, 400.0, 100.0],
        [-30.0, -400.0, 100.0], [30.0, 400.0, 100.0], [-30.0, 400.0, 100.0],
    ];
    let mut tris = Vec::new();
    tris.push([
        floor[0][0], floor[0][1], floor[0][2], floor[1][0], floor[1][1], floor[1][2],
        floor[2][0], floor[2][1], floor[2][2],
    ]);
    tris.push([
        floor[3][0], floor[3][1], floor[3][2], floor[4][0], floor[4][1], floor[4][2],
        floor[5][0], floor[5][1], floor[5][2],
    ]);
    TriangleSoup { tris }
}

#[test]
fn beam_walk_centerline_and_end_fall() {
    let soup = beam_soup();
    let dt = 1.0 / 60.0;
    let mut ctl = Controller::new([8.0, 0.0, 130.0]);
    ctl.yaw = std::f32::consts::FRAC_PI_2; // facing +Y, along the beam
    for _ in 0..40 {
        ctl.step(&soup, &idle(), dt);
    }
    assert!(ctl.grounded && (ctl.pos[2] - 100.0).abs() < 1.5, "landed on the beam: {:?}", ctl.pos);
    assert!(ctl.beam.is_some(), "beam state engaged");
    assert!(ctl.pos[0].abs() < 1.5, "snapped to the centerline: {}", ctl.pos[0]);

    // Walk along +Y with a constant sideways push: the
    // centerline lock absorbs the lateral input.
    let walk = MoveInput { vel_xy: [25.0, 150.0], jump: false, drop: false };
    for _ in 0..60 {
        ctl.step(&soup, &walk, dt);
    }
    assert!(ctl.beam.is_some(), "still on the beam");
    assert!(ctl.pos[1] > 120.0, "travelled along the beam: {}", ctl.pos[1]);
    assert!(ctl.pos[0].abs() < 6.0, "lateral push absorbed: {}", ctl.pos[0]);
    assert!((ctl.pos[2] - 100.0).abs() < 1.5, "beam height kept");

    // Walk off the far end: beam clears, he falls.
    for _ in 0..240 {
        ctl.step(&soup, &walk, dt);
        if !ctl.grounded {
            break;
        }
    }
    assert!(!ctl.grounded, "walked off the beam end");
    assert!(ctl.beam.is_none(), "beam state cleared");
    let z0 = ctl.pos[2];
    for _ in 0..20 {
        ctl.step(&soup, &idle(), dt);
    }
    assert!(ctl.pos[2] < z0 - 15.0, "falling past the beam end");
}


#[test]
fn quai_has_a_real_beam() {
    // Regression anchor: the narrow plank on the Quai dock
    // (found by the sector beam scan, 2026-10-23).
    let bf = BigFile::open(BF).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains("100_Quai"))
        .expect("quai entry");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let soup = TriangleSoup::from_world(&world);
    let g = soup.ground_height(-351.0, 1.0, 1.0e5, 0.0).expect("floor at the plank");
    let span = soup.beam_span([-351.0, 1.0, g], [1.0, 0.0]);
    assert!(span.is_some(), "the Quai plank should read as a beam");
    let span = span.unwrap();
    assert!(span.width < 40.0, "plank is narrow: {}", span.width);
}
