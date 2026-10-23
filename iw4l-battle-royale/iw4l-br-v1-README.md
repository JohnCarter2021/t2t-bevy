# IW4L Battle Royale — v1 (Phase 0 + A)

Changes on top of vladtrc/iw4L master `acc57a5`, branch `br-phase0`.

## What changed
1. **Player cap 18 → 100**
   - `master_protocol::MAX_SESSION_MEMBERS = 100` (master server rooms, adverts, admission).
   - `IW4L_MASTER_MAX_PLAYERS` now accepts 2..=100.
   - `sv_maxclients` script dvar now follows `playerstate_iw4::MAX_CLIENTS` (= 100).
2. **Entity numbering re-based for 100 players**
   - Players: entities 0–99. Corpses: 100–199 (`MAX_CLIENT_CORPSES = 100`, one per player).
     `GENTITY_SPAWN_BASE` moves 26 → 200 automatically (it is derived, not hardcoded).
   - Corpse test in the client body renderer tightened to the corpse range
     (was `>= base`, which would mis-range at the new base).
   - Scorebar rank arrays (fixed `[i32; 18]`) replaced with a Vec — ranks now work past 18.
3. **New gametype: Battle Royale (`br`)**
   - `GameModeKind::BattleRoyale`, wire tag 8 (existing tags 0–7 unchanged), tokens
     `br` / `battleroyale` / `battle_royale`. Non-team (FFA family), DM spawn points.
   - Runs on the retail DM script base; when the mode is BR the match install pushes:
     `iw4l_br=1`, `scr_dm_numlives=1` (one life — death = spectate, no respawn),
     `scr_dm_scorelimit=0`, `scr_dm_timelimit=20` (minutes).
   - Built-in GSC module `iw4l_br/zone` (self-gates on `iw4l_br=1`, inert in other modes):
     computes the zone centre/radius from the map's DM spawns, waits for
     `prematch_over`, then shrinks in 6 phases (60s grace, then 60/45/45/30/30/20s
     steps to 70/50/35/22/12/5% of the start radius) and deals 5 damage/second to
     players outside the zone. Announces the last living player as winner.

## Build (Windows, your PC)
Follow IW4L's own `docs/WINDOWS.md` / `docs/BUILD.md` for this source tree
(Rust via rustup + GNU Make). This zip is the full source tree with the BR
changes already applied — no patching needed.

## Test 1 — cap lift (the Phase 0 measurement)
Wasteland's zone name is `mp_brecourt` in MW2 (Wasteland is the map's display
name). If it does not load, use any large open map you have and note which.

```
set IW4L_GAMETYPE=dm
make map mp_brecourt CMDS='wait world; spawn 0; force_match_start; bot add 30'
```
Then raise it: `bot add 60`, then `bot add 99` (99 bots + you = 100 clients).
Watch/record:
- Does the match stay stable at 31 / 61 / 100 clients?
- Frame/tick time (see `docs/PERF.md` — `IW4L_PERF` traces) at each step.
- Any errors in the log mentioning entity numbers, corpses, or snapshot size.

## Test 2 — BR mode on Wasteland
```
set IW4L_GAMETYPE=br
make map mp_brecourt CMDS='wait world; spawn 0; force_match_start; bot add 20'
```
Verify (these are the open questions v1 is designed to answer):
1. Do dead players stay dead (numlives=1) and go to spectator?
2. Does the "zone starts shrinking" message appear after prematch, and does
   standing outside the zone after the first shrink deal damage?
3. When one player remains, is a winner announced? Does the match end early,
   or only at the 20-minute timelimit?
4. With `bot add 99`: does a full 100-client BR lobby run?

Send back: the run log (or its last ~100 lines) + what you observed for
Test 1 stability and Test 2 items 1–3. That decides v2 (loot, drop-in
spawn spread, smooth zone lerp, net interest filtering if tick/size blows up).

## Known v1 limits
- No loot yet — players spawn with their class loadout, like DM.
- No parachute/drop — players spawn at DM spawn points on the ground.
- Zone shrinks in steps (no smooth lerp) and its centre is the spawn centroid.
- Lobby UI gametype list comes from the retail `mp/gametypestable.csv`, which
  has no `br` row — select BR via `IW4L_GAMETYPE=br`, not the host menu.
- Scorebar/HUD cosmetics beyond the rank fix are untested at 100 players.
- Nothing here is run-verified: compile-verified on Linux only; your PC is
  the first live test (no MW2 data exists on the build VM).

## v1.1 (2026-10-22) — fix: "rendering preparation failed"

Found on the first Windows run (RTX 4060, build clean): every match died
with the Notice "rendering preparation failed". Root cause: the renderer's
pre-flight skin-support check priced its safety envelope as
(largest body ×2 + largest weapon ×7) × actors. Stock actors = 18 + 8 = 26;
v1 made it 100 + 100 = 200, so the envelope exceeded the GPU buffer limit /
resident budget and readiness refused every match before it could start.

Fix (`render_frontend` spawn.rs only): resident skinning shapes are keyed
by unique model — 100 players share the same body models — and the runtime
residency check already prices each distinct shape once. The envelope now
prices the whole distinct body/weapon/FPV catalog (a superset of what can
be resident at once). Palettes stay actor-scaled (per presented actor).

Upgrade: extract `iw4l-br-v1.1.zip` OVER your existing `C:\iw4l-br-v1`
folder (same top folder name; your `.env` and `target\` are not in the zip,
so the rebuild is incremental, ~1–2 min). You do NOT need the
`IW4L_SKIN_RESIDENT_BYTES` variable any more — the default budget is fine.
Then re-run the Step 4 smoke test, then Steps 5–6.

## v1.2 (2026-10-22) — envelope coefficients aligned with the runtime check

v1.1's log line from the test machine gave the real numbers: distinct
catalog = 5,731,461 verts (output 183,406,752 B, input 366,813,504 B,
palettes 10,385,280 B), buffer limit 4 GiB — but the stock residency
weighting `(input + palettes) ×2 + output ×3` = 1.30 GiB still exceeded
the default 1 GiB budget → ResidencyPressure.

The skinning dispatcher's own residency accounting against the same
budget is `inputs + transient ×2 + output ×2`, over presented unique
shapes only. v1.2 prices the pre-flight envelope with those coefficients
over the full distinct catalog: 754,397,568 B < 1 GiB. Extract over
`C:\iw4l-br-v1` the same way; no environment variables needed.
