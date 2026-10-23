//! LZO1X decoder — a bounds-checked Rust port of the public LZO1X
//! decompression algorithm (the control flow of Oberhumer's
//! lzo1x_decompress_safe, as used via minilzo by Jade-Toolkit).
//! Jade streams are sequences of independent LZO1X blocks; this module
//! decodes one block and checks the output length exactly.
//!
//! The decoder is an explicit state machine mirroring the reference's
//! labels: Literal (main loop) -> FirstRun -> Match -> MatchDone ->
//! Literal | MatchNext(literals).

#[derive(Clone, Copy, PartialEq)]
enum State {
    /// Main loop: read a token; >=16 is a match, <16 a literal run.
    Literal,
    /// Just copied a long literal run: read a token; <16 is the M1
    /// big-offset match (3-byte copy), >=16 a normal match.
    FirstRun,
    /// Decode the match described by the token in `t`.
    Match,
    /// Copy `n` trailing literals left over from a match, then read a
    /// token: <16 is M1 big-offset, >=16 a normal match.
    MatchNext(usize),
}

fn copy_from(out: &mut Vec<u8>, dist: usize, n: usize) -> Result<(), String> {
    if dist == 0 || dist > out.len() {
        return Err(format!(
            "lzo: bad match distance {dist} at output {}",
            out.len()
        ));
    }
    let start = out.len() - dist;
    for i in 0..n {
        let b = out[start + i];
        out.push(b);
    }
    Ok(())
}

/// Decode one LZO1X block; output must be exactly `expected_len` bytes.
pub fn decompress_block(input: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(expected_len);
    let mut ip = 0usize;
    let mut t: usize;
    let mut state: State;

    let next = |ip: &mut usize| -> Result<usize, String> {
        let b = input
            .get(*ip)
            .copied()
            .ok_or_else(|| "lzo: input overrun".to_string())?;
        *ip += 1;
        Ok(b as usize)
    };

    // First-byte shortcut: >17 encodes an initial literal run.
    t = next(&mut ip)?;
    if t > 17 {
        t -= 17;
        if t < 4 {
            state = State::MatchNext(t);
        } else {
            if ip + t > input.len() {
                return Err("lzo: initial literal overrun".into());
            }
            out.extend_from_slice(&input[ip..ip + t]);
            ip += t;
            state = State::FirstRun;
        }
    } else {
        // Reprocess the byte as a main-loop token.
        ip = 0;
        state = State::Literal;
    }

    loop {
        match state {
            State::Literal => {
                t = next(&mut ip)?;
                if t >= 16 {
                    state = State::Match;
                    continue;
                }
                if t == 0 {
                    let mut acc = 0usize;
                    loop {
                        let b = next(&mut ip)?;
                        if b != 0 {
                            acc += 15 + b;
                            break;
                        }
                        acc += 255;
                    }
                    t = acc;
                }
                let n = t + 3;
                if ip + n > input.len() {
                    return Err("lzo: literal overrun".into());
                }
                out.extend_from_slice(&input[ip..ip + n]);
                ip += n;
                state = State::FirstRun;
            }
            State::FirstRun => {
                t = next(&mut ip)?;
                if t >= 16 {
                    state = State::Match;
                    continue;
                }
                // M1 big-offset: 3-byte copy.
                let b = next(&mut ip)?;
                let dist = 0x0801 + (t >> 2) + (b << 2);
                copy_from(&mut out, dist, 3)?;
                state = match_done(input, ip);
            }
            State::MatchNext(n) => {
                if ip + n > input.len() {
                    return Err("lzo: post-match literal overrun".into());
                }
                out.extend_from_slice(&input[ip..ip + n]);
                ip += n;
                t = next(&mut ip)?;
                if t >= 16 {
                    state = State::Match;
                    continue;
                }
                let b = next(&mut ip)?;
                let dist = 0x0801 + (t >> 2) + (b << 2);
                copy_from(&mut out, dist, 3)?;
                state = match_done(input, ip);
            }
            State::Match => {
                if t >= 64 {
                    // M2 match.
                    let b = next(&mut ip)?;
                    let dist = 1 + ((t >> 2) & 7) + (b << 3);
                    let len = (t >> 5) - 1;
                    copy_from(&mut out, dist, len + 2)?;
                } else if t >= 32 {
                    // M3 match.
                    let mut len = t & 31;
                    if len == 0 {
                        let mut acc = 0usize;
                        loop {
                            let b = next(&mut ip)?;
                            if b != 0 {
                                acc += 31 + b;
                                break;
                            }
                            acc += 255;
                        }
                        len = acc;
                    }
                    if ip + 2 > input.len() {
                        return Err("lzo: m3 offset overrun".into());
                    }
                    let v = u16::from_le_bytes([input[ip], input[ip + 1]]) as usize;
                    ip += 2;
                    let dist = 1 + (v >> 2);
                    copy_from(&mut out, dist, len + 2)?;
                } else if t >= 16 {
                    // M4 match.
                    let mut dist = (t & 8) << 11;
                    let mut len = t & 7;
                    if len == 0 {
                        let mut acc = 0usize;
                        loop {
                            let b = next(&mut ip)?;
                            if b != 0 {
                                acc += 7 + b;
                                break;
                            }
                            acc += 255;
                        }
                        len = acc;
                    }
                    if ip + 2 > input.len() {
                        return Err("lzo: m4 offset overrun".into());
                    }
                    let v = u16::from_le_bytes([input[ip], input[ip + 1]]) as usize;
                    ip += 2;
                    dist += v >> 2;
                    if dist == 0 {
                        // End-of-stream marker.
                        if out.len() == expected_len {
                            return Ok(out);
                        }
                        return Err(format!(
                            "lzo: length mismatch at EOS (got {}, expected {expected_len})",
                            out.len()
                        ));
                    }
                    dist += 0x4000;
                    copy_from(&mut out, dist, len + 2)?;
                } else {
                    // M1 short match: 2-byte copy.
                    let b = next(&mut ip)?;
                    let dist = 1 + (t >> 2) + (b << 2);
                    copy_from(&mut out, dist, 2)?;
                }
                state = match_done(input, ip);
            }
        }
    }
}

/// match_done: the low 2 bits of the byte before the current position
/// (the last match-offset byte) say how many literals follow.
fn match_done(input: &[u8], ip: usize) -> State {
    let lit = (input[ip - 2] & 3) as usize;
    if lit == 0 {
        State::Literal
    } else {
        State::MatchNext(lit)
    }
}
