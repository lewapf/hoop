# Hoop

Preview machine embroidery files on Janome MC9900 hoops before you stitch them.
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
drag a rectangle on the hoop around the part to check, such as a tight corner
or small letters, and optionally sew outlines only, without fills. Save the
piece or send it to USB. The same window opens the calibration sheet: rows of
satin widths, narrow fills, fill densities, pull compensation and small
letters on one 123 × 111 mm sheet for the SQ14b hoop.

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
