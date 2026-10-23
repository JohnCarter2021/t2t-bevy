//! Jade engine BIG archive (`.bf`) reader.
//!
//! Format reverse-engineered from two independent sources that agree:
//! 1. kaminoer/Jade-Toolkit `src/BigFile.cpp` (a port of `core/bigfile.py`)
//! 2. The user's own `pop3.bf` (GOG Two Thrones), parsed byte-by-byte
//!    2026-10-22: header v38, 6,648 active files, 5 dirs, ext size 84.
//!
//! Layout (all integers little-endian u32):
//! - Header (44 B): magic `BIG\0` (`BUG\0` = encrypted, unsupported),
//!   then version, max_file, max_dir, max_key, root, first_free_file,
//!   first_free_dir, size_of_fat, num_fat, universe_key.
//! - FAT descriptors (24 B each, from offset 44): max_file, max_dir,
//!   pos_fat, next_pos_fat, first_index, last_index.
//! - At each FAT's `pos_fat`, three consecutive tables sized by
//!   `size_of_fat` (total allocated slots, may exceed used count):
//!   FileTable:    (pos, key) pairs, 8 B each
//!   FileTableExt: (length, prev, next, parent, time, name[64]) —
//!                 entry size 84 (old) or 88 (new), auto-detected
//!   DirTable:     (first_file, first_subdir, prev, next, parent,
//!                 name[64]), 84 B each

use std::collections::HashMap;
use std::path::Path;

pub const INVALID_INDEX: u32 = 0xFFFF_FFFF;
pub const INVALID_KEY: u32 = 0xFFFF_FFFF;
const MAX_LEN_NAME: usize = 64;
const HEADER_SIZE: usize = 44;
const FATDES_SIZE: usize = 24;
const FILE_ENTRY_SZ: usize = 8;
const DIR_ENTRY_SZ: usize = 84;
const FILEEXT_OLD: usize = 84;
const FILEEXT_NEW: usize = 88;

#[derive(Debug, Clone)]
pub struct BfFile {
    pub index: u32,
    pub pos: u32,
    pub key: u32,
    pub length: u32,
    pub prev: u32,
    pub next: u32,
    pub parent: u32,
    pub time: u32,
    pub name: String,
}

impl BfFile {
    pub fn is_active(&self) -> bool {
        !self.name.is_empty() && self.key != INVALID_KEY
    }
}

#[derive(Debug, Clone)]
pub struct BfDir {
    pub index: u32,
    pub first_file: u32,
    pub first_subdir: u32,
    pub prev: u32,
    pub next: u32,
    pub parent: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
struct FatDesc {
    max_file: u32,
    max_dir: u32,
    pos_fat: u32,
    first_index: u32,
}

pub struct BigFile {
    pub version: u32,
    pub max_file: u32,
    pub max_dir: u32,
    pub universe_key: u32,
    pub ext_size: usize,
    pub files: HashMap<u32, BfFile>,
    pub dirs: HashMap<u32, BfDir>,
    data: Vec<u8>,
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn name_at(b: &[u8], off: usize) -> String {
    let raw = &b[off..off + MAX_LEN_NAME];
    let end = raw.iter().position(|&c| c == 0).unwrap_or(MAX_LEN_NAME);
    // bigfile.py decodes ascii/errors='replace'; lossy UTF-8 matches for the
    // ASCII names this format requires.
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

impl BigFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<BigFile, String> {
        // Archives here are <= ~500 MB; milestone 1 keeps bytes in memory for
        // fast random access. (The toolkit streams; switch when profiles ask.)
        let data = std::fs::read(path.as_ref())
            .map_err(|e| format!("cannot read {}: {e}", path.as_ref().display()))?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<BigFile, String> {
        if data.len() < HEADER_SIZE {
            return Err("file too small for BigFile header".into());
        }
        match &data[0..4] {
            b"BIG\0" => {}
            b"BUG\0" => return Err("encrypted BigFile (BUG header) not supported".into()),
            _ => return Err("not a BigFile (bad magic)".into()),
        }
        let version = u32_at(&data, 4);
        let max_file = u32_at(&data, 8);
        let max_dir = u32_at(&data, 12);
        let root = u32_at(&data, 20);
        let _ = root;
        let size_of_fat = u32_at(&data, 32) as usize;
        let num_fat = u32_at(&data, 36);
        let universe_key = u32_at(&data, 40);

        // FAT descriptors.
        let mut fats: Vec<FatDesc> = Vec::new();
        let mut first_fat_pos: Option<usize> = None;
        for i in 0..num_fat as usize {
            let pos = HEADER_SIZE + i * FATDES_SIZE;
            if let Some(ffp) = first_fat_pos {
                if pos + FATDES_SIZE > ffp {
                    break;
                }
            }
            if pos + FATDES_SIZE > data.len() {
                break;
            }
            let d = &data[pos..pos + FATDES_SIZE];
            let e = FatDesc {
                max_file: u32_at(d, 0),
                max_dir: u32_at(d, 4),
                pos_fat: u32_at(d, 8),
                first_index: u32_at(d, 16),
            };
            if e.max_file > 10_000_000 || e.max_dir > 1_000_000 {
                break;
            }
            if first_fat_pos.is_none() {
                first_fat_pos = Some(e.pos_fat as usize);
            }
            fats.push(e);
        }
        if fats.is_empty() {
            return Err("no FAT descriptors".into());
        }

        // Detect FileTableExt entry size: with the right size, the first
        // DirTable entry (root) has parent == INVALID_INDEX and a
        // printable name.
        let ext_size = [FILEEXT_OLD, FILEEXT_NEW]
            .into_iter()
            .find(|&sz| {
                let off = fats[0].pos_fat as usize
                    + size_of_fat * FILE_ENTRY_SZ
                    + size_of_fat * sz;
                if off + DIR_ENTRY_SZ > data.len() {
                    return false;
                }
                let d = &data[off..off + DIR_ENTRY_SZ];
                u32_at(d, 16) == INVALID_INDEX && {
                    let n = name_at(d, 20);
                    !n.is_empty()
                        && n.bytes().all(|b| (0x20..=0x7e).contains(&b))
                }
            })
            .unwrap_or(FILEEXT_OLD);

        let mut files: HashMap<u32, BfFile> = HashMap::new();
        let mut dirs: HashMap<u32, BfDir> = HashMap::new();

        for fat in &fats {
            let pf = fat.pos_fat as usize;
            let fi = fat.first_index;
            let mf = fat.max_file as usize;
            let md = fat.max_dir as usize;

            // FileTable: (pos, key).
            for j in 0..mf {
                let off = pf + j * FILE_ENTRY_SZ;
                if off + FILE_ENTRY_SZ > data.len() {
                    break;
                }
                let idx = fi + j as u32;
                files.insert(
                    idx,
                    BfFile {
                        index: idx,
                        pos: u32_at(&data, off),
                        key: u32_at(&data, off + 4),
                        length: 0,
                        prev: INVALID_INDEX,
                        next: INVALID_INDEX,
                        parent: INVALID_INDEX,
                        time: 0,
                        name: String::new(),
                    },
                );
            }
            // FileTableExt.
            let ext_base = pf + size_of_fat * FILE_ENTRY_SZ;
            for j in 0..mf {
                let off = ext_base + j * ext_size;
                if off + 20 + MAX_LEN_NAME > data.len() {
                    break;
                }
                let idx = fi + j as u32;
                if let Some(e) = files.get_mut(&idx) {
                    e.length = u32_at(&data, off);
                    e.prev = u32_at(&data, off + 4);
                    e.next = u32_at(&data, off + 8);
                    e.parent = u32_at(&data, off + 12);
                    e.time = u32_at(&data, off + 16);
                    e.name = name_at(&data, off + 20);
                }
            }
            // DirTable.
            let dir_base = pf + size_of_fat * (FILE_ENTRY_SZ + ext_size);
            for j in 0..md {
                let off = dir_base + j * DIR_ENTRY_SZ;
                if off + DIR_ENTRY_SZ > data.len() {
                    break;
                }
                let idx = fi + j as u32;
                dirs.insert(
                    idx,
                    BfDir {
                        index: idx,
                        first_file: u32_at(&data, off),
                        first_subdir: u32_at(&data, off + 4),
                        prev: u32_at(&data, off + 8),
                        next: u32_at(&data, off + 12),
                        parent: u32_at(&data, off + 16),
                        name: name_at(&data, off + 20),
                    },
                );
            }
        }

        Ok(BigFile {
            version,
            max_file,
            max_dir,
            universe_key,
            ext_size,
            files,
            dirs,
            data,
        })
    }

    pub fn read_data(&self, index: u32) -> &[u8] {
        match self.files.get(&index) {
            Some(e) if e.length > 0 => {
                let start = e.pos as usize;
                let end = start + e.length as usize;
                if end <= self.data.len() {
                    &self.data[start..end]
                } else {
                    &[]
                }
            }
            _ => &[],
        }
    }

    pub fn dir_path(&self, mut idx: u32) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut guard = 0;
        while idx != INVALID_INDEX {
            match self.dirs.get(&idx) {
                Some(d) => {
                    parts.push(d.name.as_str());
                    idx = d.parent;
                }
                None => break,
            }
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        parts.reverse();
        parts.join("/")
    }

    /// Full path (dir + name) for a file entry.
    pub fn file_path(&self, e: &BfFile) -> String {
        let d = self.dir_path(e.parent);
        if d.is_empty() {
            e.name.clone()
        } else {
            format!("{d}/{}", e.name)
        }
    }

    pub fn active_files(&self) -> Vec<&BfFile> {
        let mut v: Vec<&BfFile> = self.files.values().filter(|f| f.is_active()).collect();
        v.sort_by_key(|f| f.index);
        v
    }

    pub fn active_dir_count(&self) -> usize {
        self.dirs.values().filter(|d| !d.name.is_empty()).count()
    }
}
