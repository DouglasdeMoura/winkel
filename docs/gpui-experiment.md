# The GPUI cutover

Winkel's frontend is GPUI (Zed's GPU-accelerated UI framework). This note
records the experiment that preceded the 0.2.0 cutover from Quickshell/QML.

The backend was untouched through the port: the window spawns
`winkel --backend` and speaks the same wire (`docs/protocol.md`). One screen,
three dialogs, the tab ring, the hero editor, the wheels, the keys sheet, the
theme, the catalogues — everything the QML did.

## Verdict (shipped in 0.2.0)

Pixel-close against the live QML app (0.17 % AE: rasterizer noise plus a
documented quickshell shadow-margin offset), full feature parity for
keyboard and pointer use, faster startup, same memory and idle CPU. Costs:
a larger binary, hand-rolled rendering, no AccessKit on Linux yet, no RTL
layout mirroring, and whole-frame repaints on beat events.

Harnesses kept for regression:

- `tools/gpui-perf.py` — RSS / CPU / spawn timing
- `tools/gpui-compare.sh` — historical QML-vs-GPUI pixel diff (needs both
  binaries from the experiment era; not used in CI)

See `CHANGELOG.md` for the 0.2.0 release notes.
