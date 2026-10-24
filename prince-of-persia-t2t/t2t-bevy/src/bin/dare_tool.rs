//! DARE sound archive tool: list or extract the
//! files packed in a Ubisoft DARE .big using its
//! .fat index.
//!
//! FAT format (reverse-engineered from T2T):
//! a sequence of records, no header —
//!   [u32 self_offset][u32 big_offset][u32 size]
//!   [u32 zero][u32 name_len][name bytes incl NUL]
//! record size = 20 + name_len; the entries tile
//! the .big exactly.
//!
//! Usage:
//!   dare_tool list  <sound.fat>
//!   dare_tool extract <sound.fat> <sound.big> <name-substr> <out-file>
//!   dare_tool decode <sound.fat> <sound.big> <bank-substr> <out-dir>
//!     (decode every wave record of a .sb0 bank to WAVs)
use std::process::ExitCode;
use t2t::dare::{decode_sound, decode_sound_from, parse_bank, parse_fat, write_wav, FatEntry};

type Entry = FatEntry;

fn main() -> ExitCode {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: dare_tool <list|extract> ...");
        return ExitCode::FAILURE;
    }
    let fat = std::fs::read(&a[2]).expect("read fat");
    let entries = parse_fat(&fat);
    match a[1].as_str() {
        "list" => {
            let mut total = 0u64;
            for e in &entries {
                println!("{:>12}  {:>12}  {}", e.big_offset, e.size, e.name);
                total += e.size as u64;
            }
            println!("{} entries, {} bytes total", entries.len(), total);
        }
        "extract" => {
            let big = std::fs::read(&a[3]).expect("read big");
            let e = entries
                .iter()
                .find(|e| e.name.contains(&a[4]))
                .expect("entry not found");
            let start = e.big_offset as usize;
            std::fs::write(&a[5], &big[start..start + e.size as usize]).expect("write out");
            println!("extracted {} ({} bytes) -> {}", e.name, e.size, a[5]);
        }
        "census" => {
            // dare_tool census <sound.fat> <sound.big>:
            // parse + decode every sound of every bank.
            let big = std::fs::read(&a[3]).expect("read big");
            let (mut banks, mut sounds, mut frames, mut fails) = (0u32, 0u32, 0u64, 0u32);
            for e in entries.iter().filter(|e| e.name.ends_with(".sb0")) {
                let start = e.big_offset as usize;
                let bank_bytes = &big[start..start + e.size as usize];
                let bank = match parse_bank(bank_bytes) {
                    Ok(b) => b,
                    Err(err) => {
                        println!("{}: BANK PARSE FAILED: {}", e.name, err);
                        fails += 1;
                        continue;
                    }
                };
                banks += 1;
                for s in &bank.sounds {
                    sounds += 1;
                    let decoded = if let Some(soff) = s.stream_offset {
                        let want = format!("/{}", s.name.as_deref().unwrap_or(""));
                        entries
                            .iter()
                            .find(|x| x.name.to_ascii_lowercase().ends_with(&want.to_ascii_lowercase()))
                            .map(|se| {
                                let b = (se.big_offset + soff) as usize;
                                let len = (s.sec1_len + s.sec2_len) as usize;
                                decode_sound_from(&big[b..b + len], s)
                            })
                            .unwrap_or_else(|| Err("stream file not in fat".into()))
                    } else {
                        decode_sound(bank_bytes, s)
                    };
                    match decoded {
                        Ok(pcm) => frames += (pcm.len() as u64) / (s.channels as u64),
                        Err(err) => {
                            fails += 1;
                            println!("{} sound {}: {}", e.name, s.index, err);
                        }
                    }
                }
            }
            println!(
                "census: {} banks, {} sounds, {} frames decoded, {} failures",
                banks, sounds, frames, fails
            );
        }
        "decode" => {
            let big = std::fs::read(&a[3]).expect("read big");
            let e = entries
                .iter()
                .find(|e| e.name.contains(&a[4]))
                .expect("entry not found");
            let start = e.big_offset as usize;
            let bank_bytes = &big[start..start + e.size as usize];
            let bank = parse_bank(bank_bytes).expect("parse bank");
            std::fs::create_dir_all(&a[5]).expect("create out dir");
            let tag = e
                .name
                .trim_start_matches("sound/")
                .trim_end_matches(".sb0")
                .replace('/', "_");
            for s in &bank.sounds {
                let decoded = if let Some(soff) = s.stream_offset {
                    let want = format!("/{}", s.name.as_deref().unwrap_or(""));
                    entries
                        .iter()
                        .find(|x| x.name.to_ascii_lowercase().ends_with(&want.to_ascii_lowercase()))
                        .map(|se| {
                            let b = (se.big_offset + soff) as usize;
                            let len = (s.sec1_len + s.sec2_len) as usize;
                            decode_sound_from(&big[b..b + len], s)
                        })
                        .unwrap_or_else(|| Err("stream file not in fat".into()))
                } else {
                    decode_sound(bank_bytes, s)
                };
                let pcm = match decoded {
                    Ok(p) => p,
                    Err(e) => {
                        println!(
                            "{}_{}: DECODE FAILED: {} (rate {} ch {} ima {} sec1 {} sec2 {} chunk {} off {})",
                            tag, s.index, e, s.rate, s.channels, s.ima, s.sec1_len,
                            s.sec2_len, s.chunk, s.offset_in_chunk
                        );
                        continue;
                    }
                };
                let frames = pcm.len() as u32 / s.channels;
                let path = format!("{}/{}_{}.wav", a[5], tag, s.index);
                write_wav(&path, &pcm, s.channels, s.rate).expect("write wav");
                println!(
                    "{}: {} Hz, {} ch, {} — declared {} frames, decoded {}",
                    path,
                    s.rate,
                    s.channels,
                    if s.ima { "IMA" } else { "PCM" },
                    s.frames,
                    frames
                );
            }
        }
        _ => return ExitCode::FAILURE,
    }
    ExitCode::SUCCESS
}
