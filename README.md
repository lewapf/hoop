# Hoop

Make and preview machine embroidery for Janome, Brother and Baby Lock, Pfaff
and Husqvarna Viking, Bernina, or any machine that reads DST.
Hoop opens JEF, DST, EXP, PES and PEC files, shows the design on the fabric in
the hoop, tells you whether it fits, and plays the stitch-out thread by thread.

The Lettering tab turns text into stitches: pick a font, set the height,
spacing and arc, choose a tatami or satin fill and a satin or running edge,
pick Janome threads, and save a JEF for the machine. Save as JEF also converts
an opened DST, EXP or PES file, with any rotation or mirroring baked in.

The Artwork tab turns SVG files and pictures (PNG, JPG, WebP; logos and clip
art, not photos) into designs. Pictures are reduced to a few flat colors and
traced into smooth outlines first. Every
color becomes a Janome thread, shapes are filled with any fill style, strokes
become running or satin lines, hidden parts are cut away so layers don't pile
up, and the colors can be merged down to a few threads.

The window works like a desktop editor: a menu bar and tool bar on top, the
Objects and Colors dock on the left, a tabbed properties dock on the right
(Source, Shape, Style, Threads, Hoop, Stitch, View in the Artwork tab) and a
status bar with the pointer position, size, stitches and fit. Both docks can
be resized or hidden from the View menu.

The Objects list shows every shape of the artwork, topmost first; drag a row
to restack it, click the dot to hide a shape. Pick shapes there or on the
hoop: click one, Ctrl+click to add, or drag over empty fabric to pick
several with a band. Press on any shape and drag to move it; moving shapes
snap to the edges and middles of the others (pink guides, Alt to turn it
off). Drag a corner of a single picked shape to resize it and the round knob
above it to turn it (Shift snaps to 15°). Shift+drag moves the whole design
and the middle button pans. The Shape tab and Shape menu align, restack,
duplicate, delete and recolor the picked shapes. Edit points shows its outline points: drag them, right-click
one to remove it, double-click the outline to add one; Simplify thins out
traced outlines first. Add shape puts in a circle, rounded square, heart,
star, shield, banner or line, and Start from shapes begins an empty design
built only from those.

Quick styles set up a whole look in one click, the fit card offers fixes
(rotate, a bigger hoop, scale to fit), the Threads card shows how much thread
each color needs and copies the list, and Details warns about stitches likely
to sew badly. Settings, the last lettering and recent files are remembered in
`~/.local/state/hoop/prefs`.

To combine designs, say a name under a logo, press Pin to hoop (`P`), then
make or open the next part and move it into place. Save as JEF saves every
pinned part and the current design as one file.

To try a design before sewing all of it, open **Test stitch-out** (`Ctrl+T`):
drag a box on the hoop around the part to check, such as a tight corner or
small letters (in Artwork, or box the picked shapes), and optionally sew
outlines only, without fills. While the window is open the hoop shows only
what the test will sew. Save the piece or send it to USB.

The window's Calibration sheet page opens a sheet of satin widths, narrow
fills, fill densities, pull compensation and small letters (123 × 111 mm, for
the SQ14b hoop). After sewing it, pick what looked best on each row. Hoop
remembers the answers, uses the row spacing and pull compensation for new
designs, and the Check list judges designs by your limits instead of general
rules.

## Digitizing features

These work much like the tools in digitizing programs such as Wilcom or
Janome Artistic Digitizer.

- **Satin columns that turn with the shape.** The Column fill sews a
  shape as satin along its center lines. It splits the column at sharp
  corners, shortens crowded stitches on the inside of curves and fills
  any leftovers with tatami. Auto, the default for artwork, uses columns
  for parts up to 7 mm wide and tatami for wider ones. Satin edges and
  lines also turn smoothly round corners.
- **Stitch types:**
  - Fills: tatami, satin, contour, wave, zigzag, crosshatch, gradient,
    open, stipple (a meander that never crosses itself) and cross-stitch.
  - Edges: run, triple (bean stitch), satin, blanket (an E-stitch for
    appliqué) and scallop.
- **Underlay and fabrics.** The underlay can be auto, edge run, tatami,
  zigzag or double zigzag. Fabric presets (woven, knit, denim, towel,
  fleece, felt, leather, silk, cap) set the row spacing, pull compensation,
  stitch length and underlay together.
- **Stitch angles and start points.** In the Shape tab, under Stitching,
  drag a line across a shape to set its fill direction. Several lines
  split the fill into parts, each sewn at its own angle. A start point
  sets where the needle enters the shape.
- **Drawing.** In the Source tab, under Draw, put a picture faintly behind
  the hoop and trace it with the pen:
  - Click to place a corner.
  - Drag to pull out a curve.
  - Click the first point to close a filled shape.
  - Press Enter to end a line.
- **Lettering layouts:** straight, arc, circle badge (line 1 round the
  top, line 2 round the bottom), arch, valley, bulge, pinch, bridge,
  wave, grow and monogram.
- **Team names.** Type one name per line and Hoop saves each one as its
  own JEF.
- **Templates.** The Key fob and Coaster quick styles make in-the-hoop
  pieces with a stop for the backing fabric. Lace makes free-standing
  lace on water-soluble stabilizer.
- **Editing stitch files.** For an opened JEF, DST, EXP, PES or PEC file:
  - Right-click a thread in Colors to sew it earlier or later, or to
    delete it.
  - Merge neighbouring threads of the same color, or remove stitches
    under 0.3 mm.
  - Pick single stitches on the hoop to move or delete them.
- **Several hoopings.** File → Split into hoopings cuts a design that is
  too big for the hoop into numbered parts. Each part starts with
  registration crosses for lining up the fabric.
- **Other machines.** File → Export as writes DST, EXP, PES, PEC and VP3.
- **Worksheet.** Ctrl+P saves a PDF with the thread sequence, the sewing
  time and an actual-size template for placing the design.
- **Library.** The Library dock shows a thumbnail of every design in a
  folder.
- **Thread charts.** View → Thread charts names the closest thread from
  Madeira, Isacord, Robison-Anton, Sulky and other brands. The charts are
  downloaded from the Ink/Stitch project. Tick the spools you own under
  My threads, and Hoop prefers those.

For a screen recording, `hoop --demo` (or File → Play demo) plays a
30-second tour with captions.

Hoop follows the Omarchy theme. It never overwrites the file you opened unless
you pick that file in the save dialog yourself.

## Hoops

| Hoop  | Sewing area   |          |
|-------|---------------|----------|
| SQ14b | 140 × 140 mm  | Standard |
| RE20b | 170 × 200 mm  | Standard |
| SQ20b | 200 × 200 mm  | Optional |
| FA10a | 100 × 40 mm   | Free arm |

## Shortcuts

| Key                     | Action                                     |
|-------------------------|--------------------------------------------|
| `O`                     | Open a file                                |
| `V` / `L` / `A`         | Viewer / lettering / artwork               |
| `Ctrl+S`                | Save as JEF                                |
| `Ctrl+U`                | Send to the machine's USB stick            |
| `Ctrl+T`                | Test stitch-out: an area, outlines, or the calibration sheet |
| `Ctrl+V`                | Paste SVG code as artwork                  |
| `Ctrl+A`                | Pick every artwork shape                   |
| `Arrows` · `[` / `]`    | Picked shapes: move / one step up or down  |
| `Shift+[` / `Shift+]`   | Picked shapes: to the back / front         |
| `Ctrl+D` · `Del` · `Ctrl+H` · `Esc` | Picked shapes: duplicate / delete / hide / deselect |
| `Ctrl+Shift+S`          | Save a picture of the preview              |
| `Ctrl+P`                | Save a worksheet PDF with an actual-size template |
| `Space`                 | Play or pause the stitch-out               |
| `Home` / `End`          | Back to the first stitch / show everything |
| `[` / `]`               | Previous / next thread                     |
| `,` / `.`               | Step one stitch back / forward             |
| Arrows                  | Move the design 1 mm (`Shift`: 10 mm)      |
| `R` / `Shift+R`         | Rotate 90° right / left                    |
| `M`                     | Mirror                                     |
| `P`                     | Pin the design to combine it with the next |
| `C`                     | Center in the hoop                         |
| `1` – `4`               | Pick a hoop                                |
| `F` / `D`               | Show the whole hoop / zoom to the design   |
| `G` / `J` / `T`         | Grid / jump stitches / thread texture      |
| `H`                     | Thread density view                        |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo any change                   |
| `?`                     | Show the shortcuts                         |
| `Q`                     | Quit                                       |

With the mouse: drag the design to move it, drag the fabric to pan, scroll to
zoom, double-click to show the whole hoop. Drop a file on the window to open it.

## Build

```sh
bin/build      # build/hoop
bin/test       # unit tests
bin/install    # makepkg -fsi, adds Hoop to the app launcher
```

Thread names come from the Janome and Brother charts stored in JEF and PES
files. DST and EXP files don't store colors, so Hoop shows placeholders you can
change by clicking a swatch.

## Lettering tips

- Contour fill stitches rings that follow each letter's outline inward.
- Appliqué patches stop the machine twice: lay the fabric on the placement
  line, then trim it next to the tack-down before the satin cover.
- The Patch card puts a box, rounded box, oval or circle behind the text with
  a satin border, for badges and name patches.

- Satin fill suits strokes up to about 8 mm wide; use tatami for bolder letters.
- Keep text at least 8–10 mm tall for clean small letters, and use a running
  edge rather than satin below that.
- Row spacing of 0.4 mm suits most fabrics; go up to 0.5 mm on towels with a
  water-soluble topper, down to 0.35 mm for dense, shiny satin.

## Fonts

Bundled display fonts (Alfa Slab One, Anton, Bebas Neue, Bungee, Dancing
Script, Lobster, Pacifico, Righteous) come from Google Fonts under the SIL Open
Font License (`assets/fonts/OFL.txt`). Installed system fonts are listed too.

## Checking a design before it sews

The Check tab in the left dock looks at the stitches of whatever is on the
hoop, opened files included, and lists what would sew badly:

- **Density:** thread laid per mm², measured in 1 mm cells. One layer of
  tatami or satin is about 2–2.5 mm of thread per mm²; around four layers or
  more is a problem (needle breaks, puckering). Press `H` to color the dense
  and overloaded spots on the hoop.
- **Needle hits** piling up in one spot, which can cut the fabric.
- **Stitch lengths:** stitches under 0.3 mm (thread breaks) and over 8 mm
  (snags).
- **Shape widths** in artwork: fills under about 1 mm can't be filled (fix:
  stitch as outline), tatami under 2.5 mm looks better as satin, satin over
  about 8 mm should be tatami.
- **Lettering:** letters under 5 mm, small letters with a row fill, wide
  satin edges, very close fill rows.

Each issue has Show (zooms to it and selects the shape) and, where there is
one, a fix button. Save and Send to USB ask before writing a design that
still has problems.

The stitch engine also avoids common digitizing faults on its own: it drops
stitches under 0.3 mm, splits long steps between fill rows, caps satin lines
at 7 mm, and lays a sparse cross-direction underlay, inset 0.5 mm, under
fills wider than 3 mm. The Fabric box in Style sets row spacing, pull
compensation and stitch length for woven, knit, denim or towel and says which
stabilizer to use. Every artwork shape can have its own stitch type (Shape
tab → Stitch as).

## Artwork tips

- Send to USB writes the JEF into `EMB/Embf` on the stick with an 8-letter
  name, and Eject closes the stick so it can be pulled out.
- For pictures, set Colors to the number of colors in the logo plus one for
  the background, and raise Clean for scanned or blurry images.

- Flat-color logos work best. Gradients become one thread in their average
  color; embedded pictures are skipped (trace them into paths first).
- A full-size white rectangle behind the drawing is treated as background and
  starts switched off.
- Each thread can be stitched its own way (Stitch as: satin, open, outline…),
  so a logo's text can be satin while its background stays tatami.
- Keep "Cut hidden parts" on for layered logos, and raise "Detail" to drop
  specks too small to stitch cleanly.

## Translating

Hoop is in English, and every line it shows can be translated. Translations
are gettext `.po` files, so any PO editor works (Poedit, Lokalize, or a text
editor).

1. Copy `locales/hoop.pot` to `<language>.po`, for example `pl.po`.
2. Fill in each `msgstr`. Fill in the `Language:` and `Language-Name:` header
   lines too, for example `pl` and `Polski`.
3. Put the file in `~/.local/share/hoop/locale/`. Then pick the language in
   View → Language. Lines left empty stay in English.

Some lines have placeholders in braces, such as `{name}` or `{w:.1}`. Keep
them as they are. You can move them anywhere in the sentence. Lines with a
count (`msgid_plural`) have one `msgstr[k]` for each plural form of the
language. Hoop knows the plural rules of the common languages, Polish and
Russian included.

To ship a finished translation with Hoop:

1. Add it to `locales/`.
2. List it in `BUNDLED` in `src/i18n.rs`.

After changing UI text, refresh the template with
`cargo test --release update_pot -- --ignored`.

The worksheet PDF uses the PDF base fonts. These have no letters such as ł
or ś, so the PDF prints them without their accents.

## License and credits

Hoop is released under the [MIT License](LICENSE). Copyright (c) 2026 Pawel
Furman.

Hoop was written by Pawel Furman with
[Claude Code](https://claude.com/claude-code), Anthropic's AI coding agent.
Bundled fonts, shapes and tool bar icons keep their own licenses, listed
above and in `assets/`. The tool bar icons are from
[Phosphor Icons](https://phosphoricons.com) (MIT, `assets/icons/LICENSE`).
