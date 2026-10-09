# Hoop handoff

Notes for whoever picks this project up next, human or agent. Updated
2026-10-07, at commit `26ec189` plus this file.

## What it is

Hoop is a native Rust desktop app for Omarchy (Arch + Hyprland) that previews
and creates machine embroidery for a **Janome MC9900**. It replaces paid
digitizing software for the owner's needs. Three tabs:

- **Viewer (V):** opens JEF, DST, EXP, PES, PEC; shows the design on the MC9900
  hoops (SQ14b 140×140, RE20b 170×200, SQ20b 200×200, FA10a 100×40) with a fit
  check, rotate/mirror/move, thread colors, stitch-out playback, details.
- **Lettering (L):** text from bundled or system fonts into stitches. Fill
  styles, edges, arc, slant, alignment, quick styles, patches (incl. appliqué).
- **Artwork (A):** SVG files, pasted SVG code, or traced pictures (PNG/JPG/WebP)
  into stitches, one Janome thread per color.

Output: **Save as JEF** (Ctrl+S), **Send to USB** (Ctrl+U, writes to
`EMB/Embf`), **Save picture** (Ctrl+Shift+S, PNG of the preview). **Pin to hoop**
(P) combines several designs into one JEF.

## Status

- Installed as the Arch package `hoop` (`/usr/bin/hoop`, launcher entry, icon,
  MIME type `application/embroidery` for jef/dst/exp/pes/pec, set as default).
- 36 unit tests pass (`bin/test`), 3 ignored sample exporters (see Testing).
- **Nothing made by Hoop has been stitched on the machine yet** except the
  JEF format itself: the writer reproduces the header of `heart_5x5_fill.jef`,
  which was stitched successfully before Hoop existed. The owner was asked
  to sew a sample sheet; their feedback should drive default tuning.
- **Send to USB has not met a real stick.** Detection is tested against sample
  `lsblk` output, writing against a temp folder.

## Layout

```
src/main.rs       eframe setup: app id "hoop", MSAA 4, vsync OFF (see Gotchas)
src/app.rs        app state, canvas, panels' contents (big; egui immediate mode)
src/app/shell.rs  menu bar, tool bar, status bar, left/right docks and tabs
src/app/shapes.rs artwork shape editing: selection, drag/snap, handles,
                  points, Arrange actions, Objects list, Shape tab
src/canvas.rs     fabric, hoop and stitch drawing; Transform (rotate/mirror/offset)
src/formats.rs    file readers, JEF writer, Pattern/Block, thread charts,
                  thread length, quality warnings
src/hoops.rs      hoop list and fit math
src/lettering.rs  text layout + the shared stitch engine (Stitcher, fills,
                  edges, contour fill, patches, presets)
src/artwork.rs    SVG → shapes (usvg), artwork stitching, thread grouping,
                  per-thread looks, presets
src/trace.rs      picture → artwork (k-means, despeckle, marching squares,
                  Chaikin, RDP, hole matching)
src/usb.rs        lsblk parsing, udisksctl mount/eject, EMB/Embf writing
src/prefs.rs      ~/.local/state/hoop/prefs (key = value lines)
src/theme.rs      Omarchy theme colors, watched from a background thread
assets/fonts/     8 bundled OFL display fonts (Google Fonts)
tests/fixtures/   heart_5x5_fill.jef (machine-tested), GEARBRMN.dst
pkgbuild/         PKGBUILD, .desktop, icon, MIME xml
tools/jef-preview.html  standalone web viewer, used for headless visual checks
```

Units everywhere: **0.1 mm, y pointing down**, design or hoop center at the
origin. JEF stores y up; the reader/writer flip it.

## Build, test, install

```sh
bin/build      # cargo build --release → build/hoop
bin/test       # cargo test
bin/install    # makepkg -fsi (needs a terminal for sudo)
```

Agents can't type the sudo password. What worked:

```sh
cd pkgbuild && makepkg -f --noconfirm && \
  pkexec pacman -U --noconfirm "$PWD/hoop-0.1.0-1-x86_64.pkg.tar.zst"; \
  rm -rf src pkg *.pkg.tar.zst
```

`pkexec` shows a graphical polkit prompt that the owner approves.
Git identity is set per repo (Pawel Furman, guessed from the email; ask if
wrong). Commits end with the Claude co-author line; nothing is pushed anywhere.

## How the stitch engine works

- `Stitcher` (lettering.rs) builds absolute points + `Cmd`s. `travel()` stitches
  hops ≤ 2.5 mm and jumps longer ones with 0.5 mm tie stitches at both ends.
  After `color_change()` the new thread is "fresh": it jumps to its first shape
  and ties in there. (An earlier bug tied the new thread at the old thread's end,
  leaving visible stubs; there is a regression test.)
- Row fills (`fill_shape`): rotate by the fill angle, scan rows with the
  nonzero or even-odd rule (`row_spans`), subtract a knockout mask, add pull
  compensation, group overlapping row segments into columns, stitch boustrophedon.
  Wave/zigzag warp the outline, scan straight, unwarp. Gradient varies row
  spacing. Crosshatch is two passes. Satin = one stitch per row, split >7 mm.
- Contour fill: rasterize at ~spacing/3, exact Euclidean distance transform
  (Felzenszwalb, `edt_1d`), marching squares (`iso_lines`) at each ring level,
  stitches ≤ 2 mm.
- Satin edges: zigzag across the outline with a running pass under it.
- JEF writer splits moves >12.7 mm, keeps zero-length stitches, writes the
  same header layout as pyembroidery (hoop code + 4 reference-hoop blocks).
  `point_count` = data bytes / 2.

## Gotchas learned the hard way

- **egui/eframe 0.36** API differs from older docs: `App::ui(&mut self, ui, frame)`,
  `egui::Panel::right(..).exact_size()`, `CentralPanel::show(ui, ..)`,
  `ctx.style_mut_of(theme, ..)`, glow vsync is `glow_options.vsync`.
- **vsync must stay off.** With vsync, a hidden window (other workspace)
  blocks in swap on Wayland, Hyprland shows "Application Not Responding" and the
  app gets killed. Nothing may repaint on a timer when idle; background threads
  call `ctx.request_repaint()` when they finish.
- **UI layout (since 2026-10-09):** Qt-style shell. `side_panel(ui, rect, tab)`
  draws one right-dock page; each mode's panel fn checks `tab`. `card` is
  drawn as a group box. Unicode symbols like ↶ ▾ ⟲ are missing from the UI
  font (they render as boxes), so tool buttons use words.
- **Side panel overflow:** any row wider than the panel makes the canvas paint
  over the panel's left edge (text looks cut off). It happened twice. Size
  sliders as `available_width() - <value box>`; use `slider_row`/`int_row`.
  Always screenshot after panel changes.
- Undo snapshots (`Snapshot`) cover hoop, placement, lettering and artwork
  settings; they commit after 0.5 s without change and with the mouse up.
- Hyprland here uses a **Lua config**: dispatch with
  `hyprctl dispatch 'hl.dsp.window.float({ action = "on" })'`,
  `hl.dsp.focus({ window = "address:0x…" })`, `hl.dsp.window.resize({ x, y })`,
  `hl.dsp.send_shortcut({ mods = "CTRL", key = "U", window = "address:…" })`.
  The old `dispatch togglefloating address:…` syntax errors out.
- Only kill test windows by **your own pid**; the owner often has the installed
  `hoop` open. A hidden test window once triggered the not-responding dialog
  on the owner's screen.
- The file picker on this machine is `com.thisisgm.flea.picker` (via the portal).
- An old Wine install of Janome Artistic Digitizer was removed at the owner's
  request; `~/.wine` still holds Carmageddon 2, leave it alone.

## Testing

- Unit tests sit next to the code (`#[cfg(test)]` in each module).
- Visual checks without touching the desktop: export JEFs with the ignored
  tests, then screenshot them in headless Chromium with `tools/jef-preview.html`:

```sh
HOOP_OUT=/tmp/x cargo test --release write_samples -- --ignored      # lettering
HOOP_OUT=/tmp/x HOOP_SVGS=a.svg:b.svg cargo test --release svg_samples -- --ignored
HOOP_OUT=/tmp/x HOOP_PICS=a.png cargo test --release picture_samples -- --ignored
chromium --headless=new --allow-file-access-from-files --user-data-dir=/tmp/x/chr \
  --window-size=1400,900 --virtual-time-budget=5000 --screenshot=/tmp/x/out.png \
  "file://$PWD/tools/jef-preview.html?file=/tmp/x/sample.jef"
```

- UI checks: run `./target/debug/hoop file`, float/resize it with the Lua
  dispatchers above, `grim -g "x,y wxh" shot.png`, then kill that pid.

## Known gaps

- Pinned parts can't be edited, only removed. Playback, Threads and Details show
  only the current part, not pinned ones.
- A combined JEF has not been saved and re-read in a test (logic is simple, but
  add a round-trip test).
- Shape editing (Artwork → Shapes) lives in `ArtSettings::shapes` as
  `ShapeEdit`s, so undo covers it. Moves keep the drawing's frame still on
  the hoop (`regenerate_art` shifts the placement; `keep_anchor` skips that
  after undo/redo). Point editing bakes the move into `own` geometry first.
  Node drag/right-click/double-click were not exercised with a real mouse by
  the agent (no pointer automation here); selection, outline and handles
  were checked by screenshot.
- Fills go one direction per shape (no shape-following satin columns); contour
  fill is the alternative.
- SVG dashes are stitched solid; embedded images in SVGs are skipped.
- Kerning uses only the legacy `kern` table, not GPOS.
- Thread chart (JEF/PEC colors) was written from memory; a few shades may be off.
- Optional MC9900 hoops (SQ20b, FA10a) should be double-checked against the manual.

## Ideas queued with the owner

Monogram templates, text on both halves of a circle (badge style), drag to
reorder in the Shapes list, per-shape fill style, Bezier handles in point editing,
Madeira/Isacord thread charts, editable pinned parts.

## Owner preferences

- Wants the UI to feel like a Qt desktop program: menus, tool bar, tabbed
  docks, status bar (asked 2026-10-09), and rich direct editing on the hoop.

- Wants things done end to end: build, install into Omarchy, commit.
- Likes the app to look polished and follow the Omarchy theme; screenshots are
  the check.
- Embroidery files and designs live in `~/Documents/Embroidery Designs/`.
