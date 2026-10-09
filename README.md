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

The Shapes card lists every shape of the artwork. Pick one there or click it
on the hoop to move, resize, turn, recolor, duplicate, hide, delete or
restack it (the top of the list is sewn last). Press on any shape and drag to
move it; drag a corner of the selected shape to resize it and the round knob
above it to turn it (Shift snaps to 15°). Shift+drag moves the whole design. Edit points shows its outline points: drag them, right-click
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
| `Ctrl+V`                | Paste SVG code as artwork                  |
| `Arrows` · `[` / `]`    | Selected artwork shape: move / restack     |
| `Ctrl+D` · `Del` · `Esc`| Selected shape: duplicate / delete / deselect |
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
