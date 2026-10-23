# IW4L Battle Royale v1 — Windows step-by-step (first time)

You need: your Windows PC, an internet connection for the first build,
and **Call of Duty: Modern Warfare 2 (2009)** installed on Steam
(the original MW2 multiplayer — not MWII 2022, not the campaign remaster).

Everything below is typed into **PowerShell** (Start menu → type
"PowerShell" → open "Windows PowerShell"). Copy-paste each command,
press Enter, wait for it to finish before the next one.

---

## Step 0 — Check MW2 is installed
1. Open Steam → Library → search "Modern Warfare 2".
2. It must be the **2009** game and **installed** (Play button, not Install).
   If it is not installed, install it first (~12 GB) and come back.
3. Remember its install folder. The usual place is:
   `C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Modern Warfare 2`
   (To check: Steam → right-click the game → Manage → Browse local files.
   A window opens — that folder is the game folder. The folder *above* it,
   usually `...\steamapps\common`, is what Step 3 needs.)

## Step 1 — Install Rust (one time only)
1. Go to https://rustup.rs in your browser, download `rustup-init.exe`, run it.
2. A black window opens. Press **1** then Enter (default install).
3. Wait until it says finished, then **close PowerShell and open a fresh one**
   (Rust is only recognised in new windows).
4. Check it worked — type:
   ```
   cargo --version
   ```
   You should see a version like `cargo 1.9x.x`. If you get an error,
   send me a screenshot of it.

## Step 2 — Check the C++ build tools (you probably already have these)
You installed Visual Studio 2026 for the Urban Reign project, so this is
likely done. Rust needs its C++ linker:
1. Open the Windows Start menu and search **"Visual Studio Installer"**.
2. Next to Visual Studio 2026, click **Modify**. Tick the workload
   **"Desktop development with C++"** if it is not already ticked,
   then let it install. If it was already ticked, just close the installer.
(No Visual Studio at all? In the installer download page, take
"Build Tools for Visual Studio" instead and tick the same C++ workload.)

## Step 3 — Extract the BR build and point it at your games
1. Download `iw4l-br-v1.zip` (from the chat message where I sent it).
2. Right-click it → Extract All → extract to **`C:\iw4l-br-v1`**
   (short path, no spaces — builds are happier there).
   Inside you should see folders like `crates`, `docs` and a file
   `BR-README.md`. If you see another `iw4l-br-v1` folder inside,
   use that inner one as the folder for all steps below.
3. Open PowerShell and go into the folder:
   ```
   cd C:\iw4l-br-v1
   ```
4. Tell IW4L where your games live (use YOUR Steam path from Step 0 if
   it differs — this is the folder that *contains* the MW2 folder):
   ```
   Set-Content .env "IW4L_GAMES=C:\Program Files (x86)\Steam\steamapps\common"
   ```

## Step 4 — First build + smoke test (do this before BR)
This first build downloads libraries and compiles everything.
**Expect 20–45 minutes** on your laptop, plugged in. Later runs take seconds.

Still in the same PowerShell window (inside `C:\iw4l-br-v1`):
```
cargo run --profile play -p launcher -- map mp_boneyard --cmds 'wait world; spawn 0; force_match_start; bot add 3'
```
- `mp_boneyard` is Scrapyard — small, loads fast, proves the base game works.
- The game window opens, the map loads (first load is slow — it converts
  and caches the game data), you spawn in with 3 bots.
- Walk around, shoot a bot. If that works, the install is good.
- To quit: click back into the PowerShell window, type `quit` and Enter
  (the game reads typed commands from that window while it runs).

If anything fails, **do not close the PowerShell window** — copy the red
error text at the end, or send me a screenshot, and I will tell you the fix.

## Step 5 — The 100-player cap test (Phase 0 measurement)
Same window:
```
$env:IW4L_GAMETYPE = "dm"
cargo run --profile play -p launcher -- map mp_brecourt --cmds 'wait world; spawn 0; force_match_start; bot add 30'
```
`mp_brecourt` is Wasteland. Once you are in and it feels stable, click the
PowerShell window and type, one at a time:
```
bot add 60
```
then
```
bot add 99
```
99 bots + you = 100 players. Note how it feels at 31 / 61 / 100:
smooth? stutter? crash? Then `quit`.

## Step 6 — Battle Royale test
```
$env:IW4L_GAMETYPE = "br"
cargo run --profile play -p launcher -- map mp_brecourt --cmds 'wait world; spawn 0; force_match_start; bot add 20'
```
Play and check the four things BR v1 is meant to prove:
1. When you die, you should **stay dead / spectate** (no respawn).
2. About a minute after the match starts, a message says the zone will
   shrink — then standing far from the map centre should start
   **hurting you** (5 damage per second).
3. Kill bots down to one survivor — is a **winner announced**?
4. Does the match end on its own, or only at the 20-minute limit?

## Step 7 — Send me the results
1. In `C:\iw4l-br-v1` there is now a folder `iw4l-artifacts\logs`
   (created by the game). Send me the **newest log file** from it.
2. Plus a few lines from you: did Step 4 work, how did 100 players feel,
   and the answers to the Step 6 checks 1–4.

That log + your answers are what I build v2 from (loot, drop-in spawns,
and the big map).
