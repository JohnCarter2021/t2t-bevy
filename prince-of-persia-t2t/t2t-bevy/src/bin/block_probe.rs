//! Debug: reproduce t2t_game's bank-003 blockSound load.
//! usage: block_probe <sound.big> <sound.fat>
use std::process::ExitCode;
use t2t::dare::{decode_sound, parse_bank, parse_fat};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let entries = parse_fat(&std::fs::read(&args[2]).expect("fat"));
    for e in &entries {
        if e.name.contains("003") {
            println!("fat entry: {:?} off={} size={}", e.name, e.big_offset, e.size);
        }
    }
    let ent = entries
        .iter()
        .find(|x| x.name == "sound/003.sb0")
        .or_else(|| entries.iter().find(|x| x.name.contains("003.sb0")));
    let ent = match ent {
        Some(e) => e,
        None => {
            println!("NO 003 entry found");
            return ExitCode::SUCCESS;
        }
    };
    let data = std::fs::read(&args[1]).expect("big");
    let bank = &data[ent.big_offset as usize..(ent.big_offset + ent.size) as usize];
    let b3 = parse_bank(bank).expect("parse");
    println!(
        "bank parsed: {} sounds, {} containers, tableA={}",
        b3.sounds.len(),
        b3.containers.len(),
        b3.events.len()
    );
    let hits: Vec<_> = b3.events.iter().filter(|(id, _)| (0x170000..0x170200).contains(id)).collect();
    println!("events in 0x170000..0x170200: {}", hits.len());
    for (id, tb) in hits.iter().take(12) { println!("  event {:#x} -> tb {}", id, tb); }
    println!("has 0x170024: {:?}", b3.events.iter().find(|(id, _)| *id == 0x170024));
    for (i, ch) in b3.containers.iter().filter(|(i, _)| *i >= 24 && *i <= 30) { println!("  container tb {} -> {:?}", i, ch); }
    println!("sound indices around 26..30: {:?}", b3.sounds.iter().filter(|s| (26..=30).contains(&s.index)).map(|s| (s.index, s.id)).collect::<Vec<_>>());
    let resolved = b3.resolve_event(0x170024);
    println!("resolve_event(0x170024) -> {} sounds", resolved.len());
    for s in &resolved {
        match decode_sound(bank, s) {
            Ok(pcm) => println!("  id {:#x} tb={} frames={} ch={} rate={}", s.id, s.index, pcm.len(), s.channels, s.rate),
            Err(e) => println!("  id {:#x} tb={} DECODE FAIL: {e}", s.id, s.index),
        }
    }
    ExitCode::SUCCESS
}
// (appended debug below is unreachable; see block_probe2)
