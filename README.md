# Collected works — game RE & reimplementation projects

Personal projects by Mohammad Saif Khan (with Lance, my AI agent),
collected in one place. Each folder is a self-contained project.

## prince-of-persia-t2t/
A clean-room 1:1 reimplementation of *Prince of Persia: The Two
Thrones* in Rust + Bevy (`t2t-bevy/`), built purely from the game
data files I own: a Jade BIG-archive reader, LZO decoder, skeletal
animation playback, character controller with wall collision, and
working traversal (ledge grab / hang / shimmy / climb, beam-walk
mechanics) — verified by an automated test suite on the real game
data. `NOTES.md` is the running research log; `t2t-bevy/docs/`
holds the reverse-engineered format specs and roadmap.
No game data is included in this repository.

## iw4l-battle-royale/
A 100-player battle-royale mode for the IW4L (MW2) Rust/Bevy
runtime: the 18→100 session-cap lift, entity re-basing, a BR
gametype with a shrinking zone, plus the design plan, Windows
guide, a GSC probe tool, and the three BR commits as a patch
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
