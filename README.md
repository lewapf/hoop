# Hoop

Preview machine embroidery files on Janome MC9900 hoops before you stitch them.
Hoop opens JEF, DST, EXP, PES and PEC files, shows the design on the fabric in
the hoop, tells you whether it fits, and plays the stitch-out thread by thread.

The Lettering tab turns text into stitches: pick a font, set the height,
spacing and arc, choose a tatami or satin fill and a satin or running edge,
pick Janome threads, and save a JEF for the machine. Save as JEF also converts
an opened DST, EXP or PES file, with any rotation or mirroring baked in.

The Artwork tab turns SVG files (logos, clip art, icons) into designs: every
color becomes a Janome thread, shapes are filled with any fill style, strokes
become running or satin lines, hidden parts are cut away so layers don't pile
up, and the colors can be merged down to a few threads.

Hoop follows the Omarchy theme. It never overwrites the file you opened unless
you pick that file in the save dialog yourself.

## Hoops

| Hoop  | Sewing area   |          |
|-------|---------------|----------|
| SQ14b | 140 × 140 mm  | Standard |
| RE20b | 140 × 200 mm  | Standard |
| SQ20b | 200 × 200 mm  | Optional |
| FA10a | 100 × 40 mm   | Free arm |

## Shortcuts

| Key                     | Action                                     |
|-------------------------|--------------------------------------------|
| `O`                     | Open a file                                |
| `V` / `L` / `A`         | Viewer / lettering / artwork               |
| `Ctrl+S`                | Save as JEF                                |
| `Ctrl+V`                | Paste SVG code as artwork                  |
| `Space`                 | Play or pause the stitch-out               |
| `Home` / `End`          | Back to the first stitch / show everything |
| `[` / `]`               | Previous / next thread                     |
| `,` / `.`               | Step one stitch back / forward             |
| Arrows                  | Move the design 1 mm (`Shift`: 10 mm)      |
| `R` / `Shift+R`         | Rotate 90° right / left                    |
| `M`                     | Mirror                                     |
| `C`                     | Center in the hoop                         |
| `1` – `4`               | Pick a hoop                                |
| `F` / `D`               | Show the whole hoop / zoom to the design   |
| `G` / `J` / `T`         | Grid / jump stitches / thread texture      |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo a move                       |
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

- Flat-color logos work best. Gradients become one thread in their average
  color; embedded pictures are skipped (trace them into paths first).
- A full-size white rectangle behind the drawing is treated as background and
  starts switched off.
- Keep "Cut hidden parts" on for layered logos, and raise "Detail" to drop
  specks too small to stitch cleanly.
