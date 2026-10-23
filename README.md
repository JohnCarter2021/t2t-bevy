# Collected works — game RE & reimplementation projects

Personal projects by Mohammad Saif Khan (with Lance, my AI agent),
collected in one place. Each folder is a self-contained project.

## prince-of-persia-t2t/
A clean-room 1:1 reimplementation of *Prince of Persia: The Two
Thrones* in Rust + Bevy (`t2t-bevy/`), built purely from the game
data files I own: a Jade BIG-archive reader, LZO decoder, skeletal
animation playback, character controller with wall collision,
working traversal (ledge grab / hang / shimmy / climb, beam and
pole mechanics), a 12-enemy combat roster resolved from the game's
own animation banks, the sand-tank slow-motion system, and
checkpoints with void-fall recovery — all verified by an automated
test suite (34 tests) on the real game data. The reverse
engineering goes deep: the Prince's 183-entry animation bank (GAM)
is fully mapped, the clothed loading-screen body was recovered
exactly from its bind matrices, and a disassembler for the Jade VM
behavior bytecode (`jade_dis`) has cracked the record grammar —
locomotion in this engine is code-driven, which the data proves
exhaustively. `NOTES.md` is the running research log; `t2t-bevy/docs/`
holds the reverse-engineered format specs and roadmap.
No game data is included in this repository.

## iw4l-battle-royale/
A 100-player battle-royale mode for the IW4L (MW2) Rust/Bevy
runtime: the 18→100 session-cap lift, entity re-basing, a BR
gametype with a shrinking zone, drop-in spawns with pistol start
and scattered weapon loot (v2), plus the design plan, Windows
guide, a GSC probe tool, and the four BR commits as a patch
series (`br-commits.patch`, applies on upstream commit acc57a5).

## urban-reign-dlssnr/
DLSS Neural Rendering for the Urban Reign recomp (PS2Recomp):
a native NGX DLSS callsite in the PCSX2 GS present path over
DX12. The DLSS-NR commit as a patch plus the files it touches,
at their post-change state, with setup docs.

## gow3-dlssnr/
DLSS Neural Rendering bridge work for the gow3_pc port
(God of War III Remastered): the versioned Vulkan/DLSS bridge,
camera-motion, and temporal-upscaler sources (v2–v29 iterations)
and the diagnostic patch scripts that produced them.

---
Game data, proprietary SDK DLLs, and build artifacts are
deliberately excluded; these folders hold only my own source,
patches, and documentation.
