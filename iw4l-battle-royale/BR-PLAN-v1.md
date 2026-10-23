# IW4L — 100-player Battle Royale + very big map: feasibility & plan v1
Checked against vladtrc/iw4L @ acc57a5 (master, 2026-10-22), cloned to `~/workspace/iw4l-br/iw4L`.

## The one fact that changes everything
IW4L is **not IW4x**. It is a standalone Rust runtime (Bevy + wgpu) that reads
retail MW2/MW3/BO FastFiles and converts them to its own IR. There is:
- no Radiant, no `.map` → `.d3dbsp` pipeline, no custom-map loading from disk;
- no retail binary to patch — every limit below is a Rust constant we can change.

So a big map cannot be "added" the IW4x way. It has to be **generated in code**
(see Route B) or a retail map has to be reused (Route A).

## Verified blockers for 100 players (all in source, all fixable)
| # | Blocker | Where | Fix |
|---|---------|-------|-----|
| 1 | Session cap **18** | `crates/master_protocol/src/lib.rs:19` `MAX_SESSION_MEMBERS: u8 = 18`; enforced in `net/transport/master.rs` (`IW4L_MASTER_MAX_PLAYERS must be 2..=18`) and `iw4l-master` room join | Raise to 100; audit every `u8`/array sized from it |
| 2 | Player entity numbering assumes 18 | `crates/playerstate_iw4/src/third_person.rs`: `PLAYER_CORPSE_ENTITY_BASE = 18`, `MAX_CLIENT_CORPSES = 8`, `GENTITY_SPAWN_BASE = 26` | Re-base to 100 / corpses ~100 or pooled; touches `bots/roster.rs` (`MAX_CLIENT_SLOTS = PLAYER_CORPSE_ENTITY_BASE`), `net/transport/meta_wire.rs` corpse validation, snapshot code |
| 3 | No net interest management | `crates/net/src/transport/delta.rs`: every client's delta encodes **all** players in the snapshot, every tick | Add distance/PVS filter per client before delta encode (render DPVS exists in `dpvs_iw4`, net does not use it). Without this: 100 players ≈ 100× the per-client snapshot cost of an 18p match — this is the real scaling work, not the cap |
| 4 | Entity budget | 2,047 entity numbers (`ENTITYNUM_WORLD = 0x7FF`), `MAX_SCRIPT_ENTITIES = 2048-64` (`sim/script/host/entities.rs`) | Ground loot for 100p BR = thousands of items → loot must be lightweight world-objects (`net/transport/world_object_wire.rs` / `world_sync.rs` already exist), not full gentities, or the budget is blown before gameplay |
| 5 | Bot nav on a huge map | Bot navigation is a ClipMap-baked graph, cached by content digest (`docs/MAP-LOAD.md`, `docs/BOTS.md`) | A 1–2 km² map bakes slow / graph explodes; needed only for bot-filled testing, not for 100 humans |

## The map — three routes
### A — Prototype on a retail map (recommended first)
Use the largest open retail map IW4L already loads (MW2 **Wasteland** —
the Brecourt remake, wide open fields) and lift the caps + add BR rules.
Proves #1–#4 with zero map work. Not "very big" — it's a proving ground.

### B — Synthetic generated big map (the real deliverable)
A new crate/generator that builds a `PreparedMatch` **without a FastFile**:
- terrain: heightfield mesh (~2,000 m × 2,000 m; CoD units are inches, so
  ≈ 78,700 units across — f32 coords and the existing trace code handle this),
  one ground material/texture set reused from retail;
- collision: ClipMap built from the same heightfield + box brushes for buildings;
- content: instanced retail static models (houses, crates, towers, trees from
  Rust/Terminal/Derail asset pools) placed by the generator — towns as clusters;
- entities: 100+ drop/spawn points, loot world-objects, `mp_global_intermission`.
This is the only route to a genuinely BR-sized map in this engine, and it's
possible *because* the world products (`asset_world`: GfxWorld draw + ClipMap)
are plain Rust structs, not a signed retail format.

### C — Stitch retail maps together — NOT recommended
`load_prepared_match` loads exactly one zone's world + clip. Merging two
GfxWorlds/ClipMaps (lighting, DPVS cells, materials) is the most fragile
option and still only gives you ~2 small maps.

## The BR mode itself (new gamemode, pattern exists)
Gametypes are native Rust in `crates/gamemode_iw4/` (ffa, dom, dd…) + GSC host
(`sim/script/host/iw4_gametype.rs`). A `br` module on the FFA base needs:
1. Lobby/warmup → drop: 100 spawn points spread over the map, fall/parachute
   to ground (`sim/spawn.rs` `ground_spawn` exists);
2. No respawn; death → spectator; corpse/loot drop (`sim/item.rs`, `corpse.rs`);
3. Loot: weapons/ammo/attachments as world-objects on a spawn table (see #4);
4. Shrinking zone: timer + centre/radius steps, damage outside —
   `gamemode_iw4/radius_damage.rs` is a reusable starting point;
   zone shown on compass/HUD (`asset_world/compass_map.rs`, `hud_iw4`);
5. Win = last player alive → `end_game.rs` pattern; placement + kill scoring
   in `sim/score.rs`.
6. Later: duos/squads (teams exist in `gamemode_iw4/teams.rs`), circle
   final-zone logic, killfeed/spectate polish.

## Suggested order of work
1. **Phase 0 — cap lift spike:** 18 → 100 in master_protocol + entity bases,
   local match with 99 bots on Wasteland, measure tick time + snapshot bytes.
   This tells us if #3 (interest management) is needed before anything else.
2. **Phase 1 — BR rules** on the retail map (drop, loot-lite, zone, no respawn).
3. **Phase 2 — synthetic map generator v1:** flat-ish 1 km², 4 towns, collision,
   100 spawns, loot points. Playtest, then grow terrain/towns.
4. **Phase 3 — scale hardening:** per-client snapshot filtering, loot as
   world-objects at scale, master/server tuning for 100 real clients.

## Status — v1 SHIPPED 2026-10-22 (branch `br-phase0`, commit 3d32825, `iw4l-br-v1.zip`)
Done and compile-verified on Linux (all touched crates `cargo check` clean;
BR zone GSC compiles with IW4L's own GSC compiler + IW4 catalog via a probe):
- Cap: `MAX_SESSION_MEMBERS` 18→100; `sv_maxclients` derives from the new
  `playerstate_iw4::MAX_CLIENTS = 100`.
- Entities: players 0–99, corpses 100–199 (`MAX_CLIENT_CORPSES = 100`),
  `GENTITY_SPAWN_BASE` 26→200 (derived); corpse range check in the client
  body renderer tightened; scorebar rank arrays → Vec.
- BR gametype: `GameModeKind::BattleRoyale` (wire tag 8, tokens br/battleroyale),
  DM script base + install-time dvars (`iw4l_br=1`, `scr_dm_numlives=1`,
  scorelimit 0, timelimit 20 min) + built-in `iw4l_br/zone` GSC: spawn-centroid
  zone, 6 shrink phases, 5 dmg/s outside, winner announcement.
NOT done / untested (needs his PC + MW2 data): live stability at 100,
whether numlives=1 ends the match at 1 alive or only at timelimit, loot,
drop spawns, net interest filtering (Phase 3, if measurements demand it),
Phase 2 synthetic map.

## Honest limits
- Gameplay in IW4L is already "incomplete, expect bugs/desyncs" (README) at
  18 players; 100 will expose sim/net issues that only a Phase 0 test can size.
- No MW2 copy / game data on this VM, so nothing here is run-verified yet —
  all findings are source-verified only. First live test happens on your PC
  (same pattern as your other builds: versioned zips, you build/run).
