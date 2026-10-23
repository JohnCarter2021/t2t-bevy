//! M6 foundation: collision soup + actor placement on real data.

use std::path::PathBuf;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::collision::TriangleSoup;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn quai() -> Option<Vec<u8>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let bf_path = std::env::var("T2T_POP3_BF").map(PathBuf::from).unwrap_or_else(|_| {
        root.join("../incoming/extracted/Prince of Persia The Two Thrones/pop3.bf")
    });
    let bf = BigFile::open(bf_path.to_str()?).ok()?;
    let e = bf.active_files().into_iter().find(|e| e.name.contains("100_Quai"))?;
    decompress_entry(bf.read_data(e.index)).ok()
}

#[test]
fn quai_collision_and_spawn() {
    let Some(data) = quai() else {
        eprintln!("pop3.bf not present; skipping");
        return;
    };
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let soup = TriangleSoup::from_world(&world);
    eprintln!("soup tris: {}", soup.tri_count());
    assert!(soup.tri_count() > 10_000);

    // Kaileena's placed geos (her actor GAO + rig visuals).
    for p in &world.placed {
        if p.gao_name.contains("Kaileena") {
            let mut mn = [f32::INFINITY; 3];
            let mut mx = [f32::NEG_INFINITY; 3];
            for v in p.world_vertices.chunks_exact(3) {
                for i in 0..3 {
                    mn[i] = mn[i].min(v[i]);
                    mx[i] = mx[i].max(v[i]);
                }
            }
            eprintln!("placed {}: AABB {mn:?}..{mx:?}", p.gao_name);
        }
    }

    // Ground under the FAKE_Kaileena stand-in (her level spot).
    let (sx, sy) = (-375.1f32, 94.1f32);
    let g = soup.ground_height(sx, sy, 500.0, 30.0);
    eprintln!("ground at FAKE Kaileena spot ({sx}, {sy}): {g:?}");
    assert!(g.is_some(), "floor must exist under her spot");
    let gz = g.unwrap();
    assert!((gz - 57.0).abs() < 5.0, "floor near the FAKE base z=57, got {gz}");

    // Controller: drop her 300 units above the floor; she must
    // land exactly on it and stay grounded.
    let mut ctl = t2t::controller::Controller::new([sx, sy, gz + 300.0]);
    let dt = 1.0 / 60.0;
    for _ in 0..180 {
        ctl.step(&soup, &t2t::controller::MoveInput::default(), dt);
    }
    eprintln!("after fall: pos {:?} grounded {}", ctl.pos, ctl.grounded);
    assert!(ctl.grounded, "must land");
    assert!((ctl.pos[2] - gz).abs() < 0.5, "must rest on the floor");

    // Walk east for a second; she should move and stay grounded
    // (or fall cleanly if the dock ends — report, don't guess).
    let x0 = ctl.pos[0];
    let input = t2t::controller::MoveInput { vel_xy: [150.0, 0.0], jump: false, drop: false };
    for _ in 0..60 {
        ctl.step(&soup, &input, dt);
    }
    eprintln!("after walk: pos {:?} grounded {}", ctl.pos, ctl.grounded);
    assert!(ctl.pos[0] > x0 + 100.0, "must advance east");

    // Wall resolution must not disturb her (valid) spawn spot.
    let r = soup.resolve_walls([sx, sy], 25.0, gz, 175.0);
    assert!((r[0] - sx).abs() < 1.0 && (r[1] - sy).abs() < 1.0, "spawn pushed: {r:?}");
}

fn room_soup() -> TriangleSoup {
    let floor = [
        [-1000.0, -1000.0, 0.0, 1000.0, -1000.0, 0.0, 1000.0, 1000.0, 0.0],
        [-1000.0, -1000.0, 0.0, 1000.0, 1000.0, 0.0, -1000.0, 1000.0, 0.0],
    ];
    // Wall plane x = 0, y in [-500, 500], z in [0, 300].
    let wall = [
        [0.0, -500.0, 0.0, 0.0, 500.0, 0.0, 0.0, 500.0, 300.0],
        [0.0, -500.0, 0.0, 0.0, 500.0, 300.0, 0.0, -500.0, 300.0],
    ];
    TriangleSoup { tris: floor.into_iter().chain(wall).collect() }
}

#[test]
fn wall_head_on_and_slide() {
    use t2t::controller::{Controller, MoveInput};
    let soup = room_soup();
    let dt = 1.0 / 60.0;

    // Head-on: must stop one radius from the wall.
    let mut ctl = Controller::new([-100.0, 0.0, 0.0]);
    ctl.grounded = true;
    for _ in 0..60 {
        ctl.step(&soup, &MoveInput { vel_xy: [200.0, 0.0], jump: false, drop: false }, dt);
    }
    assert!((ctl.pos[0] + 25.0).abs() < 2.0, "head-on stop: {:?}", ctl.pos);
    assert!(ctl.grounded && ctl.pos[2].abs() < 0.01);

    // Diagonal: blocked in x, slides fully in y.
    let mut ctl = Controller::new([-100.0, 0.0, 0.0]);
    ctl.grounded = true;
    for _ in 0..60 {
        ctl.step(&soup, &MoveInput { vel_xy: [200.0, 150.0], jump: false, drop: false }, dt);
    }
    assert!((ctl.pos[0] + 25.0).abs() < 2.0, "slide stop: {:?}", ctl.pos);
    assert!((ctl.pos[1] - 150.0).abs() < 5.0, "slide advance: {:?}", ctl.pos);
}
