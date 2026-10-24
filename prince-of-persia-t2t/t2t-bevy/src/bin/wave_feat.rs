//! Waveform character for one DARE event: duration, envelope,
//! zero-crossing rate, spectral centroid — used to tell a dull
//! flesh thud from a noisy scrape/clatter without listening.
//! usage: wave_feat <sound-dir> <bank-name> <event-hex>...
use std::f32::consts::PI;
use std::process::ExitCode;
use t2t::dare::{decode_sound, parse_bank, parse_fat};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let sound_dir = &args[1];
    let bank_name = &args[2];
    let fat = std::fs::read(format!("{sound_dir}/sound.fat")).expect("sound.fat");
    let big = std::fs::read(format!("{sound_dir}/sound.big")).expect("sound.big");
    let entries = parse_fat(&fat);
    let e = entries.iter().find(|e| e.name == *bank_name).expect("bank");
    let bytes = &big[e.big_offset as usize..e.big_offset as usize + e.size as usize];
    let bank = parse_bank(bytes).expect("parse bank");
    for a in &args[3..] {
        let ev = u32::from_str_radix(a.trim_start_matches("0x"), 16).expect("hex");
        for w in bank.resolve_event(ev) {
            let pcm = match decode_sound(bytes, w) {
                Ok(p) => p,
                Err(e) => {
                    println!("event {:#x} wave {:#x}: decode failed: {}", ev, w.id, e);
                    continue;
                }
            };
            // Mono-ize: left channel only if stereo (these are mono).
            let mono: Vec<f32> = pcm
                .iter()
                .step_by(w.channels.max(1) as usize)
                .map(|s| *s as f32 / 32768.0)
                .collect();
            let n = mono.len();
            let peak = mono.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let rms = (mono.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
            let third = n / 3;
            let rms3: Vec<f32> = (0..3)
                .map(|k| {
                    let seg = &mono[k * third..((k + 1) * third).min(n)];
                    (seg.iter().map(|s| s * s).sum::<f32>() / seg.len() as f32).sqrt()
                })
                .collect();
            let zcr = mono.windows(2).filter(|p| (p[0] >= 0.0) != (p[1] >= 0.0)).count() as f32
                / n as f32;
            // Spectral centroid over the first 2048 samples (naive DFT, 64 bins).
            let m = n.min(2048);
            let mut num = 0.0f32;
            let mut den = 0.0f32;
            for k in 1..64 {
                let mut re = 0.0f32;
                let mut im = 0.0f32;
                for (i, s) in mono.iter().take(m).enumerate() {
                    let ph = 2.0 * PI * k as f32 * i as f32 / m as f32;
                    re += s * ph.cos();
                    im -= s * ph.sin();
                }
                let mag = (re * re + im * im).sqrt();
                let freq = k as f32 * w.rate as f32 / m as f32;
                num += freq * mag;
                den += mag;
            }
            let centroid = if den > 0.0 { num / den } else { 0.0 };
            println!(
                "event {:#x} wave {:#x}: {} Hz {} ch {} frames ({:.2}s) peak {:.2} rms {:.3} rms-thirds [{:.3} {:.3} {:.3}] zcr {:.3} centroid {:.0} Hz",
                ev, w.id, w.rate, w.channels, n, n as f32 / w.rate as f32, peak, rms, rms3[0], rms3[1], rms3[2], zcr, centroid
            );
        }
    }
    ExitCode::SUCCESS
}
