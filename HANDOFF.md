# Hoop handoff

Notes for whoever picks this project up next, human or agent. Updated
2026-10-12 after the "digitizer features" session (see the section of that
name below).

## What it is

Hoop is a native Rust desktop app for Omarchy (Arch + Hyprland) that previews
and creates machine embroidery for a **Janome MC9900**. It replaces paid
digitizing software for the owner's needs. Three tabs:

- **Viewer (V):** opens JEF, DST, EXP, PES, PEC; shows the design on the
  chosen machine's hoops (see "Machines and the new bars") with a fit
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
src/main.rs          eframe setup: app id "hoop", MSAA 4, vsync OFF (see Gotchas)
src/stitch/          the stitch engine every design uses — add new stitch
                     types here
  mod.rs             Stitcher (ties, travel, jumps), run and satin lines,
                     row/mask spans, area and thickness
  fill.rs            FillKind, Underlay, fill_shape (rows, wave, zigzag,
                     gradient, crosshatch), contour rings, resolve (Auto)
  column.rs          turning satin columns (distance field, thinning, rays)
  special.rs         stipple, cross-stitch, blanket and scallop edges
  route.rs           travel inside a shape (A* over a grid)
src/lettering/       text: Settings (also the style every fill reads),
  mod.rs             TextShape, EdgeKind, generate, presets
  fonts.rs           bundled and system fonts, glyph outlines
  layout.rs          placement, kerning, arc, circle badge, envelopes
  patch.rs           patch backgrounds and the fob eyelet
src/artwork/         SVG and picture artwork
  mod.rs             Artwork, ArtSettings, ShapeEdit, frame helpers, presets
  svg.rs             reading SVGs into shapes and color layers
  builtin.rs         built-in shapes and the blank drawing
  sew.rs             generate: knockout, line clipping, layered order,
                     angle-line parts
  threads.rs         thread groups and merging
src/app.rs           app state, start-up, prefs, frame loop
src/app/             one file per job: canvas_view, pages, lettering_panel,
                     artwork_panel, output, usbwin, history, widgets,
                     style_cards, shell (menus, docks), shapes, checks,
                     testpiece, library, threadwin, team, tools, hoopings,
                     stitchedit, draw, settings, wizard, manual
src/check.rs         design check: density grid, needle hits, lengths, widths
src/canvas.rs        fabric, hoop and stitch drawing; Transform
src/formats.rs       readers, JEF writer, Pattern/Block, Janome/PEC charts
src/export.rs        DST, EXP, PES, PEC, VP3 writers
src/worksheet.rs     PDF worksheet
src/trace.rs         picture → artwork
src/testsew.rs       test pieces, calibration sheet, Tuning
src/split.rs         multi-hooping
src/threads.rs       thread charts (.gpl), Lab matching
src/library.rs       folder scan, thumbnails
src/i18n.rs          t!/tn!/n!, .po catalogs, plural rules
src/hoops.rs, prefs.rs, theme.rs, usb.rs
assets/fonts (lettering), assets/ui (Plex Sans), assets/shapes (Phosphor)
locales/             hoop.pot, pl.po, de.po
docs/MANUAL.md       the user manual shown by Help → User manual
tools/jef-preview.html  standalone web viewer for headless visual checks
```

Units everywhere: **0.1 mm, y pointing down**, design or hoop center at the
origin. JEF stores y up; the reader/writer flip it.

## Build, test, install

```sh
bin/build      # cargo build --release → build/hoop
bin/test       # cargo test
bin/install    # makepkg -fsi (needs a terminal for sudo)
```

Before committing: `cargo fmt` (rustfmt.toml: 160 columns) and
`cargo clippy --release --all-targets`, which is kept warning-free.

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
HOOP_SELECT=shape index, HOOP_PALETTE=query (opens Ctrl+K with it),
HOOP_BACKDROP=picture path (also turns the pen
on). Run test windows with XDG_STATE_HOME pointing at a scratch folder so
the owner's prefs aren't touched (the shell's XDG_STATE_HOME is set, so
override it explicitly).

## Later the same day

- Settings window (Edit → Settings, Ctrl+,), new project wizard (N),
  User manual (F1, `docs/MANUAL.md` built in and rendered by
  `app/manual.rs`; keep it in step with features), Phosphor shapes
  (MIT) in `assets/shapes`, installed to /usr/share/hoop/shapes.
- Polish and German catalogs in `locales/` (bundled in `i18n::BUNDLED`).
  Built from the .pot with translations kept in a scratch script; when
  adding UI text, add the PL/DE lines to the .po files too.
- UI font is IBM Plex Sans, built in (`assets/ui`); the Omarchy font stays
  for monospace. Settings → Interface font switches back.
- The owner asked about Windows/macOS. Portable already: UI, formats,
  stitching, PDF, i18n. Linux-only: usb.rs (lsblk/udisksctl), XDG/HOME
  paths, system font scan, Omarchy theme and font, curl, xdg-open,
  /usr/share paths. Plan agreed in principle, not started.

## Objects list as layers (2026-10-13)

The owner reported that restacking in Objects didn't act like layers:
with Group by thread on, `layered_order` regrouped shapes past others they
don't overlap, so the list, the sewing order and the Colors list disagreed.
Making the list the exact sewing order was measured and rejected: black
outlines hidden under fills could no longer sew after them (Bremen Mittel
went from 6 to 11 threads). Instead `ShapeEdit::pinned`: shapes dragged or
arranged in Objects are pinned and count as touching everything in
`layered_order`, so they sew exactly at their list place; others still
group. Pin icon per row (click toggles), Unpin all in the list header.
The row swatch now shows the chosen thread, not the drawing's color.
Next step agreed: Photoshop-style groups (folders) in Objects.

## Machines and the new bars (2026-10-13)

The owner asked for Hoop to stop being Janome-first and for a cleaner menu
bar. `hoops::MACHINES` lists brand families (Janome; Brother · Baby Lock;
Pfaff · Husqvarna Viking; Bernina; Other machine (DST)), each with a file
format (`export::Out`), a USB folder and its hoops. The chosen family is
a global like the language (`hoops::set_machine`, prefs key `machine`,
default Janome) and `setup.hoop` indexes its hoop list (`hoops::get`,
`hoops::list`). `Hoop::auto` marks hoops `smallest_fitting` may pick;
free-arm and "Large machines" hoops are picked by hand only. Save
(Ctrl+S), Send to USB, team names, hoopings and test pieces all write the
machine's format. No machine-specific behaviour beyond that was added.
Hoop sizes for brands other than Janome are the nominal sewing areas
from memory, not checked against manuals.

Menu bar: File, Edit, View, Design, Shape (Artwork), Machine, Help, with
the Viewer/Lettering/Artwork switch centered (`segmented`). The tool bar
is icon-only (`app/icons.rs`: Phosphor SVGs in `assets/icons`, rendered
white with resvg once and tinted), with the machine and hoop picker, the
test stitch-out and an accent "Send to machine" button on the right.
Language moved out of View (it is in Settings); thread charts moved to
Machine; Play demo to Help.

## Symbols page (2026-10-13)

The left dock has a Symbols page (`app/symbols.rs`) next to Library; the
Library lost its Shapes button. `src/symbols.rs` is the catalog: 291 (now 330)
Phosphor "fill" icons in `assets/shapes`, each with an English name (n!,
translated in the .po files), one of 11 categories and search words
(Phosphor's tags plus occasion words such as christmas, wedding, sea). A
test checks catalog and folder match both ways; add a symbol by copying
`<name>-fill.svg` from `@phosphor-icons/core` as `<name>.svg` and adding
a row. `smallest_mm` rasterizes a symbol and finds the size at which
dropping parts thinner than 1.2 mm keeps 92% of it; it runs once per
session on a background thread and drives the "Smallest first" order,
the "stitch small" filter and the tooltips. Favorites, recent and the
tile size are prefs (`symbols.favorites`, `symbols.recent`,
`symbols.tiles`). A click in Artwork goes through `add_symbol` →
`add_shapes` (shared with the built-in shapes); elsewhere it opens the
SVG. `shapes_dir()` now prefers the source tree, so a dev build never
shows the installed, older set.

Library page (same day): breadcrumbs instead of Up, Subfolders
(`library::scan_deep`, up to 3000 files), kind chips, sort (name, newest,
smallest, fewest stitches, type), S/M/L tiles (`library.tiles`), a
"Fits the <hoop>" filter and a fit dot per tile. The thumbnail painter
also returns `library::Facts` (size, stitches, colors), cached as a .txt
beside the thumbnail PNG. Sorting by size or the fit filter asks the
painter for every stitch file in the folder. VP3 files are written but
can now be read too (`formats::read_vp3`, same block layout as the
writer; checked by round trip only, not against a file from Pfaff software).

## Tool palette (2026-10-13)

`app/tools.rs` now holds the Artwork drawing tools and the palette that
floats on the canvas's top-left (egui Areas, so they take the pointer
before the canvas): Select, Edit points, Pen, Rectangle, Ellipse,
Polygon, Star, Line, Fill bucket, Stitch angle, Start point, and a
current-thread swatch (`Tools::paint`, used by `paint_layer()` for every
new shape and the bucket). `pick()`/`set_pick()` map the palette onto the
older flags (`pen.on`, `edit_points`, `tools.tool`). Keys B, U, W, K in
Artwork (`tool_keys`, run before `keys`). Shapes from the shape tools and
Line carry `ShapeEdit::made` (`artwork::Made`: rect/ellipse/polygon/star
sizes or a line's ends and bend); the Shape tab's Form card edits it and
`remake()` redraws around the old middle. `bake()` (any hand point edit)
clears it. Line ends snap within 10 px to any visible outline. All new
shapes go through `push_shape`.

## Groups in Objects (2026-10-13)

`ShapeEdit::group` holds a group id; `ArtSettings::groups` the names
(undo covers both). Members are kept next to each other in `shapes`:
`ArtSettings::gathered()` gives the order that pulls each group to its
lowest member, and `keep_groups_together()` applies it every frame the
mouse is up (and after restack/drop), also pruning empty groups. In
`sew.rs` a grouped shape counts as touching every shape outside its
group, so a group sews as one block while its members still share thread
runs. Objects draws a folder row per group (fold state `folded`,
rename `renaming` are app state, not undoable); `drop_rows` handles row
and group drags (a shape joins the group it lands in). A click on the
hoop picks the whole group; Alt picks one shape. Optimize only merges
within one group.

## Right-click menus (2026-10-13)

`app/context.rs`: `canvas_menu` (shape menu, empty-fabric menu, design
menu in Viewer/Lettering) on the canvas response, and `shape_menu` on
Objects rows too. `right_click_pick` picks what is under the pointer
(with its group) first. Shapes copy into `shape_clip` and put the marker
"hoop-shapes" on the system clipboard; Ctrl+V pastes them only when that
marker comes back as the paste text, so pasting SVG code still works.

Testing with real input: /dev/uinput is writable here. A scratch script
(uinput mouse plus `hl.dsp.cursor.move`) clicked and dragged reliably;
injected keys were not (wtype special keys and uinput keys got lost or
repeated), so keys were checked by reading code.

## Attached lines (2026-10-13)

`ShapeEdit::id` is a stable id (`fresh_id`, `fix_ids` run every frame
makes copies unique; `add_copies` gives duplicates and pastes new ids
and moves line attachments onto the copies). `Made::Line` has
`from`/`to: Option<Anchor>` (shape id plus a point in that shape's own
coordinates). `ArtSettings::follow_lines` runs every frame and moves
attached ends via `place`/`unplace`; a missing shape lets the end go.

## Frames and zodiac symbols (2026-10-13)

39 more symbols: 27 frames, badges, banners, laurels, flourishes and
dividers generated by `tools/frames.py` (run `python3 tools/frames.py
assets/shapes`), in the new Frames category, and 12 zodiac signs whose
outlines came from Noto Sans Symbols Bold (one-off ttf-parser export,
not kept). Credits are appended to assets/shapes/LICENSE. A test opens
each of them as artwork.

## Projects (2026-10-13)

`src/project.rs` is the .hoop format: JSON via serde (derives added to
the artwork, lettering, hoop and pattern types), `VERSION` 1, optional
parts `#[serde(default)]`. It stores the parsed `Artwork` itself (not the
SVG), the `ArtSettings`, or the lettering `Settings` with the font by
name, plus machine key, hoop, placement and pinned parts.
`app/project.rs`: save (in place, or a dialog for new/"as"), open
(switches machine if needed), `clean` snapshot for unsaved changes (set
by `reset_history` and on save; the font scan updates it if nothing was
touched), the title (all title setting now goes through
`project_title`; other code sets `title_base`), and `guard_close`, which
cancels a close with unsaved work and asks. `project_mode` keeps an
artwork project from being overwritten by lettering work. The Library
shows .hoop files, with stitch thumbnails for artwork projects.

Autosave: `autosave()` writes the project to `<state>/hoop/autosave.hoop`
every 30 s while there is unsaved work (`Project::origin` remembers the
project file), and deletes it when the work is clean, saved, or closed
with Don't save. `check_recovery` at start and `recovery_dialog` bring
it back (`recovered` keeps it marked unsaved until saved).

## Text in Artwork (2026-10-13)

The Text tool (`app/arttext.rs`) puts `lettering::outlines` (now public,
returns a Result) into an own Fill shape; `ShapeEdit::text`
(`artwork::ArtText`: text, font name, sizes in user units, TextShape,
arc/envelope/circle) keeps it editable. The Text card in the Shape tab
changes it and `retext` redraws around the old middle. Fonts are only
needed while editing; the outlines are stored in the shape, so projects
stitch without the font. `bake()` clears `text`.

## Fonts and Google Fonts (2026-10-13)

- 12 more bundled fonts (20 in all), static TTFs fetched from the Google
  Fonts CSS API; `FontSource::group` sorts them into Block, Serif,
  Script and Rounded for the font list. The first eight keep their
  indexes (tests and `Settings::font` default use them); new ones are
  appended. Licenses: `assets/fonts/OFL.txt` (one copyright line per
  font, then the license) and `assets/fonts/APACHE-2.0.txt`.
- `layout()` now takes the cap height from the drawn H, not the OS/2
  table: Courgette's table value made its letters about 6× too tall.
- `symbols::smallest_across(svg, px)` is `smallest_mm` at any raster
  size. Its `distance` now takes what lies past the image edge; the
  "kept" pass treats it as far from the core (before, pixels near the
  border counted as kept, which flattered thin shapes near the edge).
  `lettering::smallest_height_mm` runs it on "Hag" and converts to cap
  height. The font list shows it per font; the Text card warns when
  the height is below it.
- `lettering::render_line` rasterizes a line of text (tiny-skia) for
  font list previews. `font_file`, `face_name`, `downloaded_fonts`,
  `download_dir` (`~/.local/share/fonts/hoop`); `system_fonts` skips
  that folder and the app inserts downloaded fonts after the bundled
  ones at start-up.
- `src/gfonts.rs`: catalog from `fonts.google.com/metadata/fonts`
  (undocumented but keyless; cached a week in `~/.cache/hoop/google-fonts`,
  stale copy used offline), filtered to open, non-brand, Latin.
  Font files come from `fonts.googleapis.com/css2?family=X:wght@W`,
  which answers curl with TrueType URLs (static instances, so variable
  families work); `&text=` gives tiny preview subsets. Licenses from
  `raw.githubusercontent.com/google/fonts/main/{ofl,apache,ufl}/<dir>/`.
  All through curl, like thread charts. `thickness` is Google's stroke
  rating, or 2.5 + weight/200 where missing (most families).
- `app/fontwin.rs`: the Google Fonts window (Machine → Google Fonts,
  `HOOP_WINDOW=fonts`). Four worker threads fill previews from a LIFO
  queue (visible rows only, trimmed to 60). A shared `View` (look,
  sample text, the Lettering style, fabric, pixel height) carries a
  generation; changing it clears previews and drops stale results.
  "Stitched" runs `lettering::generate` on the subset font and draws
  with `library::draw_stitched` (edge, thread and highlight passes,
  like the hoop view). Also holds the font list's thumbnails and size
  estimates (`update_font_sizes`).
- Checked in the running app with screenshots: letters and stitched
  previews, sample text, sorting, the grouped font list. Add/remove was
  checked by the ignored network test `google_fonts_download`; the Add
  button itself wasn't clicked in a test window.
- Screenshots: `grim` geometry is in logical pixels, but the PNG is at
  the monitor scale (1.25 here), so divide image coordinates by 1.25
  before clicking. uinput clicks need a small relative move after
  `cursor.move` or egui doesn't see the hover.

## Docks, rails and the Objects panel (2026-10-13)

- Each dock's tabs are a 32 px vertical rail on the window edge (`rail`
  in shell.rs, `IconTab` = value, label, icon, badge): an icon over the
  label turned on its side (`TextShape::with_angle`, which turns around
  the galley's top-left: reading up on the left rail, down on the right),
  tab height from the label length, accent bar on the outer edge for the shown page, a red badge for
  Check problems. Clicking the shown page hides the dock (VS Code style);
  the rails replace the old chevron strips. Docks show the page name as
  a title (`dock_title`). `left_tabs()` builds the left pages per mode.
- The mode switch and the status bar facts have icons (`segmented` takes
  (value, label, icon)). Icons: 28 more Phosphor regular SVGs in
  assets/icons, registered in `icons::ICONS`; `icons::icon_button`
  returns the response (for menus under an icon).
- `app/objects.rs` now holds the Objects list (moved out of shapes.rs).
  Rows use `show_rows` (fixed 30 px), so only visible rows are drawn and
  thumbnailed. Thumbnails: `thumb_image` rasterizes `ShapeEdit::lines` in
  the thread color on the fabric color, cached per `ShapeEdit::id` with a
  hash of points, kind and color. Hovering a row sets `objects.hover`,
  which the canvas outlines like a hovered shape.
- `ShapeEdit::locked` (serde default): `shape_at` and the band pick skip
  locked shapes; the list still picks them. Alt+click on an eye solos.
  `drop_rows` moves all picked rows when a picked row is dragged.
- `HOOP_DOCK=objects` opens the Objects page for screenshots.

## Ideas queued with the owner

Bezier handles in point editing,
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
