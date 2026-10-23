//! World assembly: GAOs -> placed geometry -> textured triangles.
//!
//! Indexes a decompressed wow's records by key, parses each GAO,
//! resolves its gro_key to geometry members (directly, or through a
//! group record the toolkit way: scan the group's payload for words
//! that are themselves geometry keys), transforms vertices by the
//! GAO's global matrix, and resolves each element's texture through
//! the GAO's material (grm) chain.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use crate::gao::{parse_gao, transform_point, GaoInfo};
use crate::geometry::{parse_geometry, GeoInfo};
use crate::material::texture_key_for;
use crate::texture::{parse_texture, TexInfo};
use crate::wow::WowRecord;

pub struct RecordIndex<'a> {
    pub by_key: HashMap<u32, (Option<u32>, Range<usize>)>,
    pub data: &'a [u8],
}

impl<'a> RecordIndex<'a> {
    pub fn build(data: &'a [u8], records: &[WowRecord]) -> RecordIndex<'a> {
        let mut by_key = HashMap::new();
        for r in records {
            by_key.insert(r.key, (r.gro_type, r.data_range()));
        }
        RecordIndex { by_key, data }
    }
    pub fn get(&self, key: u32) -> Option<(Option<u32>, &[u8])> {
        self.by_key
            .get(&key)
            .map(|(g, r)| (*g, &self.data[r.clone()]))
    }
}

/// A geometry placed in world space, with per-element texture keys.
pub struct PlacedGeo {
    pub gao_name: String,
    pub geo: GeoInfo,
    /// World-space vertices (3 * nb_points), parallel to geo.vertices.
    pub world_vertices: Vec<f32>,
    /// Per element index: resolved texture key (if any).
    pub element_textures: Vec<Option<u32>>,
}

#[derive(Default, Debug)]
pub struct AssembleStats {
    pub gaos: usize,
    pub gaos_with_visual: usize,
    pub geos_placed: usize,
    pub elements: usize,
    pub elements_with_texture_key: usize,
    pub texture_keys_resolved_to_pixels: usize,
    pub texture_keys_stub_only: usize,
    pub texture_keys_missing: usize,
}

pub struct AssembledWorld {
    pub placed: Vec<PlacedGeo>,
    pub stats: AssembleStats,
    /// Texture records found in this wow, by key: (info, data range).
    pub textures: HashMap<u32, (TexInfo, Range<usize>)>,
}

/// Base-mip byte size for a texture (0 for unknown formats).
pub fn base_level_size(ti: &TexInfo) -> usize {
    let (w, h) = (ti.width as usize, ti.height as usize);
    match ti.format {
        0 => w * h * 4,
        1 => w * h,
        5 => ((w + 3) / 4).max(1) * ((h + 3) / 4).max(1) * 8,
        6 | 7 => ((w + 3) / 4).max(1) * ((h + 3) / 4).max(1) * 16,
        11 => (w * h + 1) / 2,
        _ => 0,
    }
}

fn is_geo(data: &[u8], gro: Option<u32>) -> bool {
    crate::geometry::is_geometry_entry(data, gro)
}

fn geo_group_members(
    gro_key: u32,
    idx: &RecordIndex,
    geo_keys: &HashSet<u32>,
) -> Vec<u32> {
    let Some((gro, data)) = idx.get(gro_key) else {
        return vec![gro_key];
    };
    if is_geo(data, gro) {
        return vec![gro_key];
    }
    // Group record: scan payload words for known geometry keys.
    let mut members = Vec::new();
    let mut seen = HashSet::new();
    let mut o = 0;
    while o + 4 <= data.len() {
        let v = u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
        if geo_keys.contains(&v) && seen.insert(v) {
            members.push(v);
        }
        o += 1;
    }
    if members.is_empty() {
        vec![gro_key]
    } else {
        members
    }
}

pub fn assemble(data: &[u8], records: &[WowRecord]) -> AssembledWorld {
    let idx = RecordIndex::build(data, records);
    let mut stats = AssembleStats::default();

    // Geometry key set + texture census.
    let mut geo_keys: HashSet<u32> = HashSet::new();
    let mut textures: HashMap<u32, (TexInfo, Range<usize>)> = HashMap::new();
    for r in records {
        let sub = &data[r.data_range()];
        if is_geo(sub, r.gro_type) {
            geo_keys.insert(r.key);
        }
        if let Some(ti) = parse_texture(sub) {
            textures.insert(r.key, (ti, r.data_range()));
        }
    }
    let tex_full: HashSet<u32> = textures
        .iter()
        .filter(|(_, (ti, range))| {
            let avail = range.end - range.start;
            avail >= ti.pix_start + base_level_size(ti)
        })
        .map(|(k, _)| *k)
        .collect();

    // Material lookup view for texture_key_for: key -> (gro_type, data)
    let mat_view: HashMap<u32, (u32, &[u8])> = idx
        .by_key
        .iter()
        .filter_map(|(k, (g, r))| g.map(|gro| (*k, (gro, &data[r.clone()]))))
        .collect();

    let mut placed = Vec::new();
    for r in records {
        if r.tag != ".gao" {
            continue;
        }
        let sub = &data[r.data_range()];
        let Some(gao) = parse_gao(sub) else { continue };
        stats.gaos += 1;
        if !gao.has_visual || gao.gro_key == 0 {
            continue;
        }
        stats.gaos_with_visual += 1;
        let Some(mat) = gao.global_matrix else { continue };
        let members = geo_group_members(gao.gro_key, &idx, &geo_keys);
        for member_key in members {
            let Some((_, gdata)) = idx.get(member_key) else { continue };
            let Ok(geo) = parse_geometry(gdata) else { continue };
            // Transform vertices to world space.
            let mut world = Vec::with_capacity(geo.vertices.len());
            for v in geo.vertices.chunks_exact(3) {
                let p = transform_point(&mat, [v[0], v[1], v[2]]);
                world.extend_from_slice(&p);
            }
            // Per-element texture resolution.
            let mut element_textures = Vec::new();
            let mut seen_tex: HashSet<u32> = HashSet::new();
            for (_, mat_id) in &geo.elements {
                stats.elements += 1;
                let tk = if gao.grm_key != 0 {
                    texture_key_for(gao.grm_key, *mat_id, &mat_view)
                } else {
                    None
                };
                if let Some(k) = tk {
                    stats.elements_with_texture_key += 1;
                    if seen_tex.insert(k) {
                        if tex_full.contains(&k) {
                            stats.texture_keys_resolved_to_pixels += 1;
                        } else if textures.contains_key(&k) {
                            stats.texture_keys_stub_only += 1;
                        } else {
                            stats.texture_keys_missing += 1;
                        }
                    }
                }
                element_textures.push(tk);
            }
            stats.geos_placed += 1;
            placed.push(PlacedGeo {
                gao_name: gao.name.clone(),
                geo,
                world_vertices: world,
                element_textures,
            });
        }
    }

    AssembledWorld {
        placed,
        stats,
        textures,
    }
}

impl AssembledWorld {
    /// World AABB over all placed vertices.
    pub fn aabb(&self) -> ([f32; 3], [f32; 3]) {
        let mut mn = [f32::INFINITY; 3];
        let mut mx = [f32::NEG_INFINITY; 3];
        for p in &self.placed {
            for v in p.world_vertices.chunks_exact(3) {
                for i in 0..3 {
                    mn[i] = mn[i].min(v[i]);
                    mx[i] = mx[i].max(v[i]);
                }
            }
        }
        (mn, mx)
    }
    pub fn total_tris(&self) -> usize {
        self.placed.iter().map(|p| p.geo.nb_tris()).sum()
    }
}
