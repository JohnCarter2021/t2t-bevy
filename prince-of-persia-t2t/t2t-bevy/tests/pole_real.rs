//! Pole traversal on a synthetic column: a box pillar
//! (40 x 40, z 0..700) on a floor, plus a real-data check
//! that find_pole locates the pillar and only the pillar.
use t2t::collision::TriangleSoup;
use t2t::controller::{Controller, MoveInput};

fn quad(tris: &mut Vec<[f32; 9]>, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) {
    tris.push([a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]);
    tris.push([a[0], a[1], a[2], c[0], c[1], c[2], d[0], d[1], d[2]]);
}

fn pillar_soup() -> TriangleSoup {
    let mut tris = Vec::new();
    // Floor z=0, x/y in [-600, 600].
    quad(
        &mut tris,
        [-600.0, -600.0, 0.0],
        [600.0, -600.0, 0.0],
        [600.0, 600.0, 0.0],
        [-600.0, 600.0, 0.0],
    );
    // Pillar: box x/y in [-20, 20], z 0..700 (4 side faces).
    let (w, h) = (20.0f32, 700.0f32);
    // +X face
    quad(&mut tris, [w, -w, 0.0], [w, w, 0.0], [w, w, h], [w, -w, h]);
    // -X face
    quad(&mut tris, [-w, w, 0.0], [-w, -w, 0.0], [-w, -w, h], [-w, w, h]);
    // +Y face
    quad(&mut tris, [w, w, 0.0], [-w, w, 0.0], [-w, w, h], [w, w, h]);
    // -Y face
    quad(&mut tris, [-w, -w, 0.0], [w, -w, 0.0], [w, -w, h], [-w, -w, h]);
    // Subdivide each face vertically so wall centroids
    // cluster like a real segmented mesh (find_pole buckets
    // centroids; one giant triangle per face gives n=2/cell).
    let mut tris2 = Vec::new();
    for t in &tris {
        tris2.push(*t);
    }
    let mut out = tris2;
    for seg in 0..6 {
        let z0 = seg as f32 * (h / 7.0);
        let z1 = (seg + 1) as f32 * (h / 7.0);
        quad(&mut out, [w, -w, z0], [w, w, z0], [w, w, z1], [w, -w, z1]);
        quad(&mut out, [-w, w, z0], [-w, -w, z0], [-w, -w, z1], [-w, w, z1]);
        quad(&mut out, [w, w, z0], [-w, w, z0], [-w, w, z1], [w, w, z1]);
        quad(&mut out, [-w, -w, z0], [w, -w, z0], [w, -w, z1], [-w, -w, z1]);
    }
    TriangleSoup { tris: out }
}

fn input(vel: [f32; 2], jump: bool, drop: bool, pole_move: [f32; 2]) -> MoveInput {
    MoveInput { vel_xy: vel, jump, drop, pole_move }
}

#[test]
fn pole_find_grab_climb_orbit_exit_cycle() {
    let soup = pillar_soup();
    let found = soup.find_pole([200.0, 0.0, 300.0], 260.0).expect("pillar is a pole");
    assert!((found.center[0]).abs() < 25.0 && (found.center[1]).abs() < 25.0, "center {:?}", found.center);
    assert!(found.top > 600.0 && found.bottom < 100.0, "span {}..{}", found.bottom, found.top);

    // Spawn airborne beside the pillar, drifting toward it.
    let mut ctl = Controller::new([130.0, 0.0, 320.0]);
    let dt = 1.0 / 60.0;
    for _ in 0..40 {
        ctl.step(&soup, &input([-140.0, 0.0], false, false, [0.0, 0.0]), dt);
        if ctl.pole.is_some() {
            break;
        }
    }
    let pole = ctl.pole.expect("grabbed the pillar");
    assert!((pole.center[0]).abs() < 25.0, "pole center {:?}", pole.center);
    let grab_z = ctl.pos[2];
    let radial_dist = (ctl.pos[0].powi(2) + ctl.pos[1].powi(2)).sqrt();
    assert!((radial_dist - 38.0).abs() < 3.0, "grip offset {radial_dist}");

    // Climb up for a second.
    for _ in 0..60 {
        ctl.step(&soup, &input([0.0, 0.0], false, false, [1.0, 0.0]), dt);
    }
    assert!(ctl.pole.is_some());
    assert!(ctl.pos[2] > grab_z + 70.0, "climbed {} -> {}", grab_z, ctl.pos[2]);

    // Orbit for a second: the angle must change a lot.
    let a0 = ctl.pole.unwrap().angle;
    for _ in 0..60 {
        ctl.step(&soup, &input([0.0, 0.0], false, false, [0.0, 1.0]), dt);
    }
    let a1 = ctl.pole.expect("still gripping").angle;
    assert!((a1 - a0).abs() > 1.2, "orbited {} -> {}", a0, a1);

    // Leap off: airborne, moving up, pole cleared.
    ctl.step(&soup, &input([0.0, 0.0], true, false, [0.0, 0.0]), dt);
    assert!(ctl.pole.is_none());
    assert!(!ctl.grounded && ctl.vel_z > 300.0, "leapt with vel_z {}", ctl.vel_z);

    // Re-grab on the way down, then climb back to the base
    // and step off onto the floor.
    ctl.pole_cooldown = 0.0;
    for _ in 0..90 {
        ctl.step(&soup, &input([-60.0, 0.0], false, false, [0.0, 0.0]), dt);
        if ctl.pole.is_some() {
            break;
        }
    }
    assert!(ctl.pole.is_some(), "re-grabbed after the leap");
    for _ in 0..400 {
        ctl.step(&soup, &input([0.0, 0.0], false, false, [-1.0, 0.0]), dt);
        if ctl.pole.is_none() {
            break;
        }
    }
    assert!(ctl.pole.is_none() && ctl.grounded, "stepped off at the base");
    assert!(ctl.pos[2].abs() < 2.0, "standing on the floor, z={}", ctl.pos[2]);
}

#[test]
fn flat_wall_is_not_a_pole() {
    // A single tall flat wall (a building face) must not
    // qualify: too many triangles in the cluster cells.
    let mut tris = Vec::new();
    for i in 0..40 {
        let x0 = -600.0 + i as f32 * 30.0;
        quad(
            &mut tris,
            [x0, 200.0, 0.0],
            [x0 + 30.0, 200.0, 0.0],
            [x0 + 30.0, 200.0, 700.0],
            [x0, 200.0, 700.0],
        );
    }
    let soup = TriangleSoup { tris };
    assert!(soup.find_pole([0.0, 100.0, 300.0], 260.0).is_none());
}
