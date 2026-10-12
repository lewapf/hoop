# Hoop handoff

Notes for whoever picks this project up next, human or agent. Updated
2026-10-12 after the "digitizer features" session (see the section of that
name below).

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
(P) combines several designs into one JEF. **Test stitch-out** (Ctrl+T) cuts
what's on the hoop to a dragged area and/or sews outlines only, and opens the
calibration sheet (rows A–E, see `testsew::SHEET_ROWS`). Sheet results are a
`testsew::Tuning` (prefs keys `tune.*`): `Tuning::apply` sets rows and pull
in Settings, and `check::artwork`/`check::lettering` take it for their limits.

## Status

- Installed as the Arch package `hoop` (`/usr/bin/hoop`, launcher entry, icon,
  MIME type `application/embroidery` for jef/dst/exp/pes/pec, set as default).
- 91 unit tests pass (`bin/test`), 9 ignored sample/tool tests (see
  Testing and the digitizer section).
- **Nothing made by Hoop has been stitched on the machine yet** except the
  JEF format itself: the writer reproduces the header of `heart_5x5_fill.jef`,
  which was stitched successfully before Hoop existed. The owner was asked
  to sew the calibration sheet (Test stitch-out → Open test sheet); its
  results (rows A–E) should drive default tuning and the check thresholds.
- **Send to USB has not met a real stick.** Detection is tested against sample
  `lsblk` output, writing against a temp folder.

## Layout

```
src/main.rs       eframe setup: app id "hoop", MSAA 4, vsync OFF (see Gotchas)
src/app.rs        app state, canvas, panels' contents (big; egui immediate mode)
src/app/shell.rs  menu bar, tool bar, status bar, left/right docks and tabs
src/app/checks.rs Check list, density overlay, fixes, save preflight
src/check.rs      design check: density grid, needle hits, stitch lengths,
                  artwork shape widths, lettering rules (thresholds on top)
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
src/testsew.rs    test pieces: crop stitches to an area (ties only where it
                  cuts a run), the calibration sheet
src/app/testpiece.rs  Test stitch-out window, area drag on the hoop, overlay
src/usb.rs        lsblk parsing, udisksctl mount/eject, EMB/Embf writing
src/i18n.rs       translations: t!/tn!/n! macros, .po parsing, plural rules
src/column.rs     turning satin columns (distance field, thinning, rays)
src/special.rs    stipple and cross-stitch fills, blanket and scallop edges
src/export.rs     DST, EXP, PES, PEC, VP3 writers (+ JEF via formats.rs)
src/worksheet.rs  hand-written PDF: worksheet and actual-size template
src/library.rs    design folder scan and thumbnails (cached, resvg)
src/threads.rs    thread charts from .gpl files, Lab matching, My threads
src/split.rs      multi-hooping: parts, registration crosses
src/app/library.rs, threadwin.rs, team.rs, tools.rs, hoopings.rs,
src/app/stitchedit.rs, draw.rs   the UI for the above
locales/hoop.pot  every translatable line
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
  pkexec pacman -U --noconfirm "$PWD/hoop-0.2.0-1-x86_64.pkg.tar.zst"; \
  rm -rf src pkg *.pkg.tar.zst
```

`pkexec` shows a graphical polkit prompt that the owner approves.
Git identity is set per repo (Pawel Furman, guessed from the email; ask if
wrong). Commits end with the Claude co-author line; nothing is pushed anywhere.

## How the stitch engine works

- `Stitcher` (lettering.rs) builds absolute points + `Cmd`s. `travel()` stitches
  hops ≤ 2.5 mm and jumps longer ones with 0.5 mm tie stitches at both ends.
  After `color_change()` the new thread is "fresh": it jumps to its first shape
  and ties in there.
- Travel inside a shape (`route.rs`): callers set `st.region` to a `Region`
  (the shape on a ~0.5 mm grid) around one shape's underlay, fill and edge.
  `travel()` then runs an A* path of 2.5 mm stitches through the shape
  instead of jumping. Cells cut out for dense shapes on top count as inside
  (those shapes sew later and hide the run); sewn rows are off limits on the
  top pass (`strict`), allowed under underlay. Each fill pass clears the
  sewn marks. The next fill column is the column head nearest the needle.
  This halved jumps on the Bremen crest (296 → 165, 86 of those are
  separate black outline strokes, which still jump). The JEF preview shows
  under-runs faintly through the fill; a probe confirmed every routed point
  has a later stitch within 0.3 mm. (An earlier bug tied the new thread at the old thread's end,
  leaving visible stubs; there is a regression test.)
- Row fills (`fill_shape`): rotate by the fill angle, scan rows with the
  nonzero or even-odd rule (`row_spans`), subtract a knockout mask (each
  shape on top tested on its own, `mask_spans`, so opposite windings can't
  cancel), add pull
  compensation, group overlapping row segments into columns, stitch boustrophedon.
  Wave/zigzag warp the outline, scan straight, unwarp. Gradient varies row
  spacing. Crosshatch is two passes. Satin = one stitch per row, split >7 mm.
- Artwork stitch order (`layered_order` in artwork.rs): with "group colors",
  shapes join their thread's run only past shapes they don't overlap (0.5 mm
  grid, `Coverage`); anything below a shape that it overlaps sews first. At
  each thread switch a two-level lookahead picks the next thread. Grouping
  purely by color once sewed black outlines under the fills they sat on.
- Artwork lines (strokes) with knockout on are cut where a fill drawn
  above covers them, stopping half their width plus 0.75 mm short of its
  edge (`clip_line`). Hidden outline parts then aren't sewn, and the line
  no longer overlaps that fill on the ordering grid, so black can sew in
  one run. Bremen_Wappen(Mittel).svg (fill and stroke on the same element)
  went from 9 thread blocks to 6.
  (Tried: lines under 0.8 mm as triple run. It looked bad: 2.5 mm run
  steps turn small curves into polygons, and short stitched hops between
  nearby lines show as black links. Reverted.)
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
- Check thresholds were calibrated on the owner's files: the machine-tested
  heart peaks at 4.3 mm/mm² and the commercial GEARBRMN.jef at 6.4, both
  clean; DENSE is 6.5, TOO_DENSE 9. Real stitch-outs should refine them.
  `HOOP_FILES=a.jef:b.svg cargo test --release report_samples -- --ignored --nocapture`
  prints reports for real files.
- Fills go one direction per shape (no shape-following satin columns); contour
  fill is the alternative.
- SVG dashes are stitched solid; embedded images in SVGs are skipped.
- Kerning uses only the legacy `kern` table, not GPOS.
- Thread chart (JEF/PEC colors) was written from memory; a few shades may be off.
- Optional MC9900 hoops (SQ20b, FA10a) should be double-checked against the manual.

## Digitizer features session (2026-10-12)

The owner asked for Hoop to work more like Wilcom or Janome Artistic
Digitizer, then for "all" 16 items of the plan, plus localization. All are
in, each in its own commit:

- Export DST/EXP/PES/PEC/VP3 (cross-checked by reading back with
  pyembroidery 1.5.1 in a scratch venv; PEC writes the 4-byte start jump
  PE-Design uses, so readers at offset 528 and 532 both work).
- Worksheet PDF (Ctrl+P). Base fonts only: letters outside WinAnsi lose
  their accents (`worksheet::plain`).
- Library dock with thumbnails cached in ~/.cache/hoop/thumbs.
- Thread charts: Janome built in; others are Ink/Stitch GPL palettes that
  are *downloaded* with curl into ~/.local/share/hoop/threads, never
  bundled (Hoop is MIT). JEF still stores Janome numbers.
- Turning satin (`column.rs`, FillKind::Column; FillKind::Auto picks column
  up to `column_max_mm`, default for artwork). Debug renders:
  `T=text H=mm F=font FILL=column|auto|stipple|cross EDGE=satin|blanket|...
  SHAPE=Circle|Arch|... PX=px/mm HOOP_OUT=dir cargo test --release
  column_letters -- --ignored` writes letters.png with the outline in gray.
  Known weak spots: diagonal junctions (M, N, W) get some tatami patches;
  super-bold strokes fan.
- Satin edges/lines turn round corners (`satin_rows`).
- Lettering layouts (TextShape), team names, monogram; small letters
  suggest Column.
- Stipple, cross-stitch, blanket, scallop.
- Underlay kinds and nine fabric presets.
- Angle lines and start point per artwork shape (Shape tab → Stitching).
- Multi-hooping (File → Split into hoopings).
- Stitch editing for opened files (Colors right-click, Design tab card,
  pick stitches on the hoop). Pattern edits ride in undo snapshots via an
  Rc pointer compared by address (`stitchedit::Shared`).
- Pen drawing over a backdrop picture (Source tab → Draw).
- Key fob / coaster in-the-hoop patches and free-standing lace.
- Localization: every visible line goes through `t!("English")`,
  `tn!(n, "one", "other")` for counts, `n!()` marks const tables that are
  translated where shown with `i18n::tr`. **New UI text must use these.**
  Refresh the template with `cargo test --release update_pot -- --ignored`.
  A pseudo-translation (every line in [brackets]) was used to find missed
  lines; regenerate one from hoop.pot the same way if needed.

Not exercised with a real mouse (no pointer automation here): angle-line
drag, start-point click, pen drawing, stitch picking/dragging, Colors
right-click menu. Engine parts behind them have unit tests.

Debug environment variables for screenshots (read in `debug_start`):
HOOP_MODE, HOOP_TAB (tab enum name), HOOP_DOCK=library|colors|check,
HOOP_WINDOW=threads|test|help|split, HOOP_SPLIT_HOOP=index,
HOOP_SELECT=shape index, HOOP_BACKDROP=picture path (also turns the pen
on). Run test windows with XDG_STATE_HOME pointing at a scratch folder so
the owner's prefs aren't touched (the shell's XDG_STATE_HOME is set, so
override it explicitly).

## Ideas queued with the owner

Drag to reorder in the Shapes list, Bezier handles in point editing,
editable pinned parts, motif fills, photo stitch, a real exit point (fill
order that ends where asked), turning fills that follow angle lines
smoothly instead of splitting into parts.

## Owner preferences

- Wants the UI to feel like a Qt desktop program: menus, tool bar, tabbed
  docks, status bar (asked 2026-10-09), and rich direct editing on the hoop.

- Wants things done end to end: build, install into Omarchy, commit.
- Likes the app to look polished and follow the Omarchy theme; screenshots are
  the check.
- Embroidery files and designs live in `~/Documents/Embroidery Designs/`.
