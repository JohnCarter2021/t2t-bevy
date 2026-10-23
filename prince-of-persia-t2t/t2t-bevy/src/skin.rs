//! Character skinning + clip playback.
//!
//! Ported verbatim from the verified `pose_test` pipeline:
//!   skel_global_b  = compose GAO locals (pelvis local = identity)
//!   geo_global_b   = P * skel_global_b,  P = pelvis GAO global
//!   skin matrix    = geo_global_b(posed) * bind_matrix_b
//!   bone order     = wow record order of the B_<char> GAOs
//! Vertex weights are sum-normalized (Jade MRM raw weights sum
//! to 0.25/0.5/0.75 per vertex, never 1.0).

use std::collections::HashMap;

use crate::anim::{parse_clip, read_rot5_keys, read_rot_keys, read_trans13_keys, read_trans_keys, RotKey, SectionRole, TransKey};
use crate::gao::{mat_inv, mat_mul, parse_gao, transform_point, IDENTITY};
use crate::geometry::parse_geometry;
use crate::wow::walk_wow;

pub type M = [f32; 16];

pub fn quat_mat(q: [f32; 4]) -> M {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        0.0,
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        0.0,
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

pub fn with_t(mut m: M, t: [f32; 3]) -> M {
    m[12] = t[0];
    m[13] = t[1];
    m[14] = t[2];
    m
}

/// A skinned character loaded from one wow: geometry + skin +
/// the B_<prefix> bone GAO hierarchy.
pub struct Character {
    /// (key, name, father_key, local, gao_global), record order.
    pub bones: Vec<(u32, String, u32, Option<M>, Option<M>)>,
    pub by_key: HashMap<u32, usize>,
    pub verts: Vec<f32>,
    pub faces: Vec<[u16; 7]>,
    pub uvs: Vec<f32>,
    /// Per skin-list bone: (bone_idx = GAO order index, bind matrix).
    pub skin: Vec<(u16, M)>,
    /// Per-vertex weights: (skin list position, weight).
    pub vweights: Vec<Vec<(usize, f32)>>,
    /// Pelvis GAO global matrix (the skeleton->geometry frame bridge).
    pub root_global: M,
}

impl Character {
    pub fn load(wow_data: &[u8], geo_key: u32, bone_prefix: &str) -> Option<Character> {
        let records = walk_wow(wow_data);
        let geo_rec = records.iter().find(|r| r.key == geo_key)?;
        let geo = parse_geometry(&wow_data[geo_rec.data_range()]).ok()?;
        let (_, skin_bones) = geo.skin.as_ref()?;
        let mut vweights: Vec<Vec<(usize, f32)>> = vec![Vec::new(); geo.nb_points()];
        let mut skin = Vec::new();
        for (pos, b) in skin_bones.iter().enumerate() {
            skin.push((b.bone_idx, b.bind_matrix));
            for &(v, w) in &b.weights {
                vweights[v as usize].push((pos, w as f32 / 65535.0));
            }
        }
        let mut bones = Vec::new();
        let mut by_key = HashMap::new();
        for r in &records {
            if r.tag == ".gao" {
                if let Some(g) = parse_gao(&wow_data[r.data_range()]) {
                    if g.name.starts_with(bone_prefix) {
                        by_key.insert(r.key, bones.len());
                        bones.push((r.key, g.name.clone(), g.father_key, g.local_matrix, g.global_matrix));
                    }
                }
            }
        }
        let root_global = bones.first()?.4?;
        Some(Character {
            bones,
            by_key,
            verts: geo.vertices.clone(),
            faces: geo.faces.clone(),
            uvs: geo.uvs.clone(),
            skin,
            vweights,
            root_global,
        })
    }

    pub fn bind_locals(&self) -> Vec<M> {
        self.bones.iter().map(|b| b.3.unwrap_or(IDENTITY)).collect()
    }

    /// Reconstruct the bind pose from the GEO's own bind
    /// matrices, for sectors that store their rig posed
    /// (e.g. PrinceLoading_Shape's clothed Prince). At
    /// bind, (root * G_b) * B_b = I for every skin bone;
    /// taking the root as identity gives G_b = B_b^-1
    /// directly. Locals are re-derived from those globals
    /// via the father chain; non-skin bones keep their
    /// composed globals (they do not deform this GEO).
    /// The (root, globals) pair is only defined up to a
    /// common left factor, so this is exact for any
    /// character, not just posed ones.
    pub fn reconstruct_bind(&mut self) {
        let composed = self.compose_globals(&self.bind_locals());
        let mut globals: Vec<M> = composed;
        for (bidx, bind) in &self.skin {
            if let Some(inv) = mat_inv(bind) {
                globals[*bidx as usize] = inv;
            }
        }
        let inv_globals: Vec<Option<M>> = globals.iter().map(|g| mat_inv(g)).collect();
        for i in 0..self.bones.len() {
            let father = self.bones[i].2;
            let local = if let Some(&fi) = self.by_key.get(&father) {
                match inv_globals[fi] {
                    Some(inv) => mat_mul(&inv, &globals[i]),
                    None => globals[i],
                }
            } else {
                globals[i]
            };
            self.bones[i].3 = Some(local);
            self.bones[i].4 = Some(globals[i]);
        }
        self.root_global = IDENTITY;
    }

    /// Compose skeleton globals from per-bone locals.
    pub fn compose_globals(&self, locals: &[M]) -> Vec<M> {
        fn rec(i: usize, ch: &Character, locals: &[M], cache: &mut HashMap<usize, M>) -> M {
            if let Some(m) = cache.get(&i) {
                return *m;
            }
            let father = ch.bones[i].2;
            let m = if father == 0 {
                locals[i]
            } else if let Some(&fi) = ch.by_key.get(&father) {
                let pg = rec(fi, ch, locals, cache);
                mat_mul(&pg, &locals[i])
            } else {
                locals[i]
            };
            cache.insert(i, m);
            m
        }
        let mut cache = HashMap::new();
        (0..self.bones.len()).map(|i| rec(i, self, locals, &mut cache)).collect()
    }

    /// Skin all vertices with the given per-bone skeleton globals.
    /// Returns vertices in the geometry (bind) frame.
    pub fn skin_verts(&self, skel_globals: &[M]) -> Vec<f32> {
        let mut smats: Vec<M> = Vec::with_capacity(self.skin.len());
        for (bone_idx, bind) in &self.skin {
            let g = skel_globals[*bone_idx as usize];
            let geo_g = mat_mul(&self.root_global, &g);
            smats.push(mat_mul(&geo_g, bind));
        }
        let mut out = vec![0f32; self.verts.len()];
        for v in 0..self.verts.len() / 3 {
            let p = [self.verts[3 * v], self.verts[3 * v + 1], self.verts[3 * v + 2]];
            let mut acc = [0f32; 3];
            let mut wsum = 0f32;
            for &(pos, w) in &self.vweights[v] {
                let tp = transform_point(&smats[pos], p);
                acc[0] += w * tp[0];
                acc[1] += w * tp[1];
                acc[2] += w * tp[2];
                wsum += w;
            }
            if wsum > 0.0 {
                out[3 * v] = acc[0] / wsum;
                out[3 * v + 1] = acc[1] / wsum;
                out[3 * v + 2] = acc[2] / wsum;
            } else {
                out[3 * v] = p[0];
                out[3 * v + 1] = p[1];
                out[3 * v + 2] = p[2];
            }
        }
        out
    }
}

/// A parsed 0x2002b animation clip: per-channel key streams.
pub struct Clip {
    pub rot: HashMap<usize, Vec<RotKey>>,
    pub trans: HashMap<usize, Vec<TransKey>>,
    /// Largest frame reached by any rotation stream.
    pub max_frame: u32,
}

impl Clip {
    pub fn load(record_data: &[u8]) -> Clip {
        let mut rot: HashMap<usize, Vec<RotKey>> = HashMap::new();
        let mut trans: HashMap<usize, Vec<TransKey>> = HashMap::new();
        for sec in parse_clip(record_data) {
            match sec.role {
                SectionRole::Rot => {
                    if let Some(k) = read_rot_keys(record_data, sec.offset + 18, sec.count as usize) {
                        rot.insert(sec.chan, k);
                    }
                }
                SectionRole::Rot5 => {
                    if let Some(k) = read_rot5_keys(record_data, sec.offset + 18, sec.count as usize) {
                        rot.insert(sec.chan, k);
                    }
                }
                SectionRole::Trans => {
                    if let Some(k) = read_trans_keys(record_data, sec.offset + 18, sec.count as usize) {
                        trans.insert(sec.chan, k);
                    }
                }
                SectionRole::Trans13 => {
                    if let Some(k) = read_trans13_keys(record_data, sec.offset + 18, sec.count as usize) {
                        trans.insert(sec.chan, k);
                    }
                }
                _ => {}
            }
        }
        let mut max_frame = 0u32;
        for keys in rot.values() {
            let mut t = 0u32;
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    t += k.delta as u32;
                }
            }
            max_frame = max_frame.max(t);
        }
        Clip { rot, trans, max_frame }
    }

    /// Rotation of a channel at `frame` (slot 0 is frame 0; the
    /// last key holds past the stream's span).
    pub fn sample_rot(&self, chan: usize, frame: u32) -> Option<[f32; 4]> {
        let keys = self.rot.get(&chan)?;
        let mut t = 0u32;
        let mut cur = keys[0].rotation;
        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                t += k.delta as u32;
            }
            if t > frame {
                break;
            }
            cur = k.rotation;
        }
        Some(cur)
    }
}

/// Channel -> bone assignment for a clip on a character.
pub enum ChannelMap {
    /// Channel i drives bone_idx i (NPC standard bip).
    Identity,
    /// Explicit pairs from a header record's (i, v, x) table:
    /// channel = i-1, bone_idx = v-1, 0xff = unmapped.
    Header(Vec<(usize, usize)>),
}

impl ChannelMap {
    /// Build a channel map between two rigs whose bone
    /// GAOs follow the 3ds-Max Bip naming convention
    /// ("B_<rig>_Bip L ForeArm.gao" / "B_<rig> L
    /// Forearm.gao"): clip channels are indexed by the
    /// SOURCE rig's bone order; pair each with the target
    /// bone of the same normalized name. This is what the
    /// shared enemy pool needs on rigs whose bone ORDER
    /// differs (the Babylonian Soldier keeps the bip
    /// layout but orders arms before legs and has no
    /// skirt/ribbon bones — identity mapping folds him
    /// into a knot, name mapping plays the pool cleanly:
    /// 34 of the Sand Garde's 46 channels match).
    pub fn from_bone_names(
        src: &Character,
        src_prefix: &str,
        dst: &Character,
        dst_prefix: &str,
    ) -> ChannelMap {
        fn norm(name: &str, prefix: &str) -> String {
            let s = name.strip_prefix(prefix).unwrap_or(name);
            let s = s.trim_start_matches(['_', ' ']);
            let s = s.strip_prefix("Bip").unwrap_or(s);
            let s = s.trim_start_matches(['_', ' ']);
            let s = s.strip_suffix(".gao").unwrap_or(s);
            s.trim().to_lowercase()
        }
        let mut pairs = Vec::new();
        for (ci, b) in src.bones.iter().enumerate() {
            let suf = norm(&b.1, src_prefix);
            if suf.is_empty() {
                continue;
            }
            if let Some((bi, _)) = dst
                .bones
                .iter()
                .enumerate()
                .find(|(_, db)| norm(&db.1, dst_prefix) == suf)
            {
                pairs.push((ci, bi));
            }
        }
        ChannelMap::Header(pairs)
    }

    pub fn from_header(header_data: &[u8]) -> ChannelMap {
        let pairs = header_data
            .chunks_exact(3)
            .filter(|t| t[1] != 0xff)
            .map(|t| ((t[0] - 1) as usize, (t[1] - 1) as usize))
            .collect();
        ChannelMap::Header(pairs)
    }

    pub fn pairs(&self, nbones: usize) -> Vec<(usize, usize)> {
        match self {
            ChannelMap::Identity => (0..nbones).map(|i| (i, i)).collect(),
            ChannelMap::Header(p) => p.clone(),
        }
    }
}

/// Pose a character with a clip at `frame`: bind locals with
/// the mapped channels' rotations substituted (bind
/// translations kept — clip translation streams are
/// scene-frame values, not bone locals).
pub fn pose_locals(ch: &Character, clip: &Clip, map: &ChannelMap, frame: u32) -> Vec<M> {
    let mut locals = ch.bind_locals();
    for (chan, bone) in map.pairs(ch.bones.len()) {
        if bone >= locals.len() {
            continue;
        }
        if let Some(q) = clip.sample_rot(chan, frame) {
            let bind = locals[bone];
            locals[bone] = with_t(quat_mat(q), [bind[12], bind[13], bind[14]]);
        }
    }
    locals
}

/// Full pipeline: posed vertices in the geometry frame.
pub fn posed_verts(ch: &Character, clip: &Clip, map: &ChannelMap, frame: u32) -> Vec<f32> {
    let locals = pose_locals(ch, clip, map, frame);
    let globals = ch.compose_globals(&locals);
    ch.skin_verts(&globals)
}
