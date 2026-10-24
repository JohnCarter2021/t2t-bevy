//! Debug: resolve candidate sound-event IDs in a DARE bank
//! and print what each resolves to (waves, rate, frames).
//! usage: ev_resolve <bf-dir-or-sound-dir> <bank-nnn> <event-hex>...
//! The bank bytes come from sound.big via the FAT: pass the
//! directory holding sound.big/sound.fat (the game's Sound/
//! folder) — or run from the game dir. Simpler: pass the
//! Sound dir path.
use std::process::ExitCode;
use t2t::dare::{parse_bank, parse_fat};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let sound_dir = &args[1];
    let bank_name = &args[2];
    let fat = std::fs::read(format!("{sound_dir}/sound.fat")).expect("sound.fat");
    let big = std::fs::read(format!("{sound_dir}/sound.big")).expect("sound.big");
    let entries = parse_fat(&fat);
    if bank_name == "all" {
        // Scan every FAT entry for the candidate events.
        for a in &args[3..] {
            let ev = u32::from_str_radix(a.trim_start_matches("0x"), 16).expect("hex");
            let mut found = false;
            for e in &entries {
                // .sb0 only: gamesnd.sp0 shares the 0x00150000
                // magic but is NOT a bank — parse_bank would
                // panic on its chunk table.
                if !e.name.ends_with(".sb0") {
                    continue;
                }
                let start = e.big_offset as usize;
                let end = start.saturating_add(e.size as usize);
                if end > big.len() {
                    continue;
                }
                let bytes = &big[start..end];
                let Ok(bank) = parse_bank(bytes) else { continue };
                let waves = bank.resolve_event(ev);
                if !waves.is_empty() {
                    found = true;
                    for w in waves {
                        println!(
                            "event {:#x}: bank {} wave id {:#x} {} Hz {} ch {} frames",
                            ev, e.name, w.id, w.rate, w.channels, w.frames
                        );
                    }
                }
            }
            if !found {
                println!("event {:#x}: resolves in NO bank", ev);
            }
        }
        return ExitCode::SUCCESS;
    }
    let e = entries
        .iter()
        .find(|e| e.name == *bank_name)
        .expect("bank entry by name");
    let bytes = &big[e.big_offset as usize..e.big_offset as usize + e.size as usize];
    let bank = parse_bank(bytes).expect("parse bank");
    println!("bank {}: {} events in TableA", bank_name, bank.events.len());
    for a in &args[3..] {
        let ev = u32::from_str_radix(a.trim_start_matches("0x"), 16).expect("hex");
        let waves = bank.resolve_event(ev);
        if waves.is_empty() {
            println!("event {:#x}: NOT in bank {}", ev, bank_name);
        } else {
            for w in waves {
                println!(
                    "event {:#x}: wave id {:#x} {} Hz {} ch {} frames",
                    ev, w.id, w.rate, w.channels, w.frames
                );
            }
        }
    }
    ExitCode::SUCCESS
}
