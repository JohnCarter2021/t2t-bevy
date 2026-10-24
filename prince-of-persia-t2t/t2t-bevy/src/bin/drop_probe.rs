//! Debug: drop a Controller at given spots in a sector and
//! report whether it lands. usage: drop_probe <bf> <sector>
//! <x> <y> <z> [steps]
use std::process::ExitCode;
use t2t::assemble::assemble;
use t2t::bigfile::BigFile;
use t2t::collision::TriangleSoup;
use t2t::controller::{Controller, MoveInput};
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bf = BigFile::open(&args[1]).expect("archive");
    let entry = bf
        .active_files()
        .iter()
        .find(|f| f.name.contains(&args[2]))
        .expect("entry")
        .clone();
    let data = decompress_entry(bf.read_data(entry.index)).expect("decompress");
    let records = walk_wow(&data);
    let world = assemble(&data, &records);
    let soup = TriangleSoup::from_world(&world);
    let (x, y, z): (f32, f32, f32) = (
        args[3].parse().unwrap(),
        args[4].parse().unwrap(),
        args[5].parse().unwrap(),
    );
    let steps: usize = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!(
        "ground at ({x},{y}) ref 400: {:?}; ref {z}: {:?}",
        soup.ground_height(x, y, 400.0, 30.0),
        soup.ground_height(x, y, z, 30.0)
    );
    let mut ctl = Controller::new([x, y, z]);
    let input = MoveInput {
        vel_xy: [0.0, 0.0],
        jump: false,
        drop: false,
        pole_move: [0.0, 0.0],
    };
    for i in 0..steps {
        ctl.step(&soup, &input, 0.025);
        if i % 20 == 0 || ctl.grounded {
            println!(
                "step {i}: pos {:?} grounded={} hang={} beam={}",
                ctl.pos,
                ctl.grounded,
                ctl.hanging.is_some(),
                ctl.beam.is_some()
            );
            if ctl.grounded {
                break;
            }
        }
    }
    println!("final: pos {:?} grounded={}", ctl.pos, ctl.grounded);
    ExitCode::SUCCESS
}
