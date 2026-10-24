//! wow_rec — dump one wow record's payload by key, for
//! payload archaeology (portal GAOs, sector objects).
//! Usage: wow_rec <bf> <sector-substr> <key-hex> [max-bytes]
//! Prints header fields, a hexdump of the body, and any
//! printable ASCII runs (>=4) with offsets.
use t2t::bigfile::BigFile;
use t2t::jade_stream::decompress_entry;
use t2t::wow::walk_wow;

fn main() {
    let bf_path = std::env::args().nth(1).expect("usage");
    let substr = std::env::args().nth(2).expect("usage");
    let key = u32::from_str_radix(std::env::args().nth(3).expect("usage").trim_start_matches("0x"), 16)
        .expect("key hex");
    let max: usize = std::env::args()
        .nth(4)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4096);
    let bf = BigFile::open(&bf_path).expect("open bf");
    let e = bf
        .active_files()
        .into_iter()
        .find(|e| e.name.contains(&substr))
        .expect("sector not found");
    let data = decompress_entry(bf.read_data(e.index)).expect("decompress");
    let r = walk_wow(&data)
        .into_iter()
        .find(|r| r.key == key)
        .expect("record not found");
    println!(
        "sector={} key=0x{:08x} size={} tag={:?} gro_type={:?} body_off={}",
        e.name, r.key, r.size, r.tag, r.gro_type, r.body_off
    );
    let range = r.data_range();
    let body = &data[range.start..range.end.min(range.start + max)];
    println!("body_len={} (showing {})", range.end - range.start, body.len());
    for (i, chunk) in body.chunks(16).enumerate() {
        let off = i * 16;
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let asc: String = chunk
            .iter()
            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
            .collect();
        println!("{off:06x}  {hex:<48}  {asc}", hex = hex.join(" "));
    }
    // ASCII runs with offsets
    let mut run = Vec::new();
    let mut start = 0;
    for (i, &b) in body.iter().enumerate() {
        if (0x20..0x7f).contains(&b) {
            if run.is_empty() {
                start = i;
            }
            run.push(b);
        } else {
            if run.len() >= 4 {
                println!("str@{start:06x}: {}", String::from_utf8_lossy(&run));
            }
            run.clear();
        }
    }
    if run.len() >= 4 {
        println!("str@{start:06x}: {}", String::from_utf8_lossy(&run));
    }
}
