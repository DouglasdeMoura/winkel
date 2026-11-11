# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.2.1] - 2026-11-11

### Changed

- The GPUI frontend builds without warnings. Dead fields, helpers and imports
  left over from the QML port are gone; the window size is declared once in
  `layout.rs`; the wheel's snap duration is one constant; and the layout test
  now checks the resolved band positions rather than removed intermediate
  fields.

## [0.2.0] - 2026-11-06

### Changed

- The interface is GPUI instead of Quickshell/QML. One `winkel` binary opens
  the window and still spawns itself as `--backend` on the same wire; the
  audio path stays std-only aside from cpal. Packaging no longer depends on
  Quickshell. Startup is faster; memory and idle CPU stay about the same.
- Theme switches are watched by polling `theme.name` (about half a second)
  rather than an inotify on that file.

### Removed

- The Quickshell/QML frontend and its offscreen UI test harness.

### Known gaps versus 0.1.x

- No AccessKit / screen-reader roles on Linux yet (keyboard use is unchanged).
- Pseudo-RTL and RTL layout mirroring are not implemented; cosmic-text still
  shapes bidi runs correctly inside a left-to-right layout.

## [0.1.6] - 2026-09-19

### Changed

- The time signature, subdivision and keys dialogs follow Flea's current
  cards. The window dims by half behind an open card, and a scroll over the
  dimmed ground no longer reaches the controls beneath it. Titles are bold
  at the body size; the Notes label is bold and tracked. A subdivision tile
  takes the shell's own hover and selected fills from `shell.toml`'s
  `[controls]`. The keys sheet gains the settings header, `esc` beside a
  close mark at the hit floor, and draws each key in a hairline cap.
- The README's dialog screenshots are retaken.

## [0.1.5] - 2026-09-16

### Changed

- The transport's play and stop marks are Phosphor's own icons, fill weight,
  the family the app icon's metronome already comes from. Drawn as paths, so
  they still do not depend on a font carrying the mark, and still centred by
  mass rather than by box: measured in Phosphor's 256 grid, the play
  triangle's centroid sits at (127.6, 128) and the stop square's at
  (128, 128) — the triangle's box leans right of centre precisely so the
  shape does not.

### Fixed

- The README's screenshots are retaken. They still showed the meter as it
  was before 0.1.4 marked the beat being played. `tools/screenshots.sh`
  takes them now, offscreen and the same twice, so they can keep up.

## [0.1.4] - 2026-09-16

### Added

- The beat meter marks the beat being played: a hairline over that beat's
  column, the ladder's smallest step thick, standing on the same gap the
  bars keep between themselves. The mark is clear of the fill, so a silent
  beat and a high one announce themselves alike — the beat is happening
  either way, it just makes no sound.

### Changed

- The playing beat no longer lights its empty bars. That hint rode on the
  headroom a beat had left, so it was three bars of change on a silent beat
  and nothing at all on a high one, whose bars are already full; and at 12%
  to 25% of the foreground it read at about 2:1 against the background,
  below what peripheral vision catches — which is how a metronome's meter
  is read.

## [0.1.3] - 2026-09-16

### Fixed

- The transport's play and stop marks are drawn rather than typed. A font's
  ▶ and ■ are not centred on their own ink: the triangle sat 3.3 px left and
  1 px high of the circle's centre, the square 1 px high. Drawn, the triangle
  rests on its centroid and the square on its middle, so both are exactly
  centred at any size, and neither depends on the font carrying the mark.

## [0.1.2] - 2026-09-15

### Fixed

- Dialog titles, the tempo marking and the other captions were drawn in the
  theme's muted colour, which most Omarchy themes set too faint to read: 32
  of the 39 themes installed here fell below WCAG AA, as low as 1.3:1. Each
  caption is now lifted toward the foreground only as far as it takes to
  reach 4.5:1 on the surface behind it, so a theme that already reads well
  keeps its own ink.

### Changed

- The README's screenshots are retaken with the readable captions.

## [0.1.1] - 2026-09-15

### Added

- Screenshots in the README, in the Tokyo Night and Catppuccin Latte themes.
- AUR publishing from GitHub: every release now updates `winkel`,
  `winkel-bin` and `winkel-git`, and `packaging/aur/set-version.sh` and
  `packaging/aur/publish.sh` do the same from a local machine.

### Changed

- The README is reorganised and rewritten for clarity, with a licence
  section and a floating-window recipe for Hyprland.
- The release recipes carry real checksums.

## [0.1.0] - 2026-09-15

The first release.

### Added

- Winkel, a sample-accurate metronome with a Rust audio backend and a Quickshell
  interface that follows the live Omarchy theme.
- Tempo from 10 to 400 BPM with steppers, typing, the arrow keys and tap
  tempo, and the classical tempo marking.
- A time signature editor for 1 to 16 counts over 1, 2, 4 or 8.
- Per-beat voices: silent, low, medium or high.
- Subdivisions in musical notation, from eighths and triplets to dotted
  rhythms, quintuplets and sextuplets, with any figure turned into a rest.
- A keys sheet, full keyboard control, and a single running instance.
- An inaudible keep-alive floor so Bluetooth earbuds never drop a tick.
- Translation support with plural rules, right-to-left layouts and
  pseudo-locales for testing.
- Packaging: AUR packages, release tarballs, `make install`, a desktop entry,
  an icon and AppStream metainfo.
