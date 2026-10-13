# Hoop user manual

Hoop prepares machine embroidery for home embroidery machines: Janome,
Brother and Baby Lock, Pfaff and Husqvarna Viking, Bernina, or any machine
that reads DST. It opens embroidery
files, turns text, SVG drawings and pictures into stitches, checks how a
design will sew, and writes the file the machine reads. Nothing here needs
the internet except downloading extra thread charts.

## Getting started

The quickest way in is the **new project wizard**: press `N` or pick
**File → New project…**. It asks, one step at a time:

1. **What to make:** a name, a monogram, a round badge, a key fob, a
   coaster, a lace ornament, a logo or picture, or a shape from the
   library.
2. **What fabric:** this sets row spacing, pull compensation, stitch
   length and underlay, and tells you which stabilizer to use.
3. **The content:** your text, initials, a file, or a library shape.
4. **The size and the hoop.**
5. **Thread colors.**
6. **Create:** the design appears on the hoop. The last page offers
   **Save**, **Send to USB** and **Save worksheet**, with
   step-by-step sewing advice.

You can change everything afterwards in the panels on the right.

## The window

- **Menu bar** at the top: File, Edit, View, Design, Shape (in Artwork),
  Machine and Help, with the three workspaces (Viewer, Lettering,
  Artwork) in the middle.
- **Tool bar** under it: new, open, save, worksheet; undo and redo;
  rotate, mirror, center, pin to hoop; whole hoop, zoom to design, play.
  On the right: the **machine and hoop** picker, the test stitch-out, and
  **Send to machine**.
- **Left dock:**
  - **Library:** your designs folder, with thumbnails.
  - **Objects:** artwork shapes; this page shows only in the Artwork tab.
  - **Colors:** the threads in sewing order.
  - **Check:** problems found in the design.
- **Canvas:** the fabric in the hoop. Drag the design to move it, drag
  the fabric to pan, scroll to zoom, double-click to see the whole hoop.
- **Right dock:** the settings of the current tab, in pages (Text, Style,
  Hoop, Stitch, View…).
- **Collapsing:** the small arrow next to each panel's tabs folds it
  away to a thin strip; click the arrow on the strip, or press `F9` /
  `F10`, to bring it back.
- **Status bar:** pointer position, size, stitch count, whether it fits,
  and the result of the design check.

Hoop has three tabs, switched with `V`, `L` and `A`:

- **Viewer:** opens finished embroidery files.
- **Lettering:** turns text into stitches.
- **Artwork:** turns SVG drawings and pictures into stitches.

## Opening and viewing designs

Open JEF, DST, EXP, PES, PEC or VP3 files with `O`, by dropping them on the
window, or by clicking a thumbnail in the **Library**.

- **Library:**
  - The path at the top is clickable: click a folder name to go there,
    or **…** for the folders further up. **Folder…** picks another
    folder.
  - **Subfolders** also lists the designs in the folders inside this
    one. **Find by name** filters the list.
  - **All**, **Embroidery**, **SVG** and **Pictures** show one kind of
    file.
  - Sort by name, newest first, smallest design first, fewest stitches
    first or file type. **S**, **M** and **L** set the tile size.
  - **Fits the …** shows only embroidery that fits the hoop, as it is or
    turned a quarter. On each embroidery tile, a green dot means it fits
    the hoop and an orange one that it doesn't. The size is printed under
    the name. The tooltip adds stitches, colors, file size and when it
    changed.
- **Symbols:** the left panel's **Symbols** page holds 330 ready-made
  silhouettes, sorted into categories (animals, plants, celebrations,
  sport, travel and more). **Frames & badges** has rings and frames to
  put a name or motif in, badges and patches, banners for a word, laurel
  wreaths, corner flourishes and dividers. Added in Artwork, a frame is
  sized to go round what is already drawn. The zodiac signs are under
  Signs & symbols.
  - **Find** searches names and words like "Christmas", "wedding" or
    "sea", in English or the language in use.
  - Click a category to show only it. **Favorites** keeps the symbols you
    starred (the star in a tile's corner), and **Recent** the last ones
    used.
  - The list sorts by category, A to Z, or **Smallest first**: bold
    symbols that still stitch cleanly when small. **Only ones that stitch
    small** hides the ones that need more than 25 mm. A symbol's tooltip
    says from what size it stitches cleanly.
  - **S**, **M** and **L** set the tile size.
  - In Artwork, a click adds the symbol to the drawing on the picked
    shape's thread. Elsewhere it opens as a new design. Right-click for
    both.
- **Playback:**
  - `Space` plays the stitch-out; the **Stitch** page sets the speed.
  - `,` and `.` step one stitch back or forward.
  - `[` and `]` jump to the previous or next thread.
- **Hoops:** `1`–`4` pick the machine's first four hoops. The
  **Hoop** page says whether the design fits and offers fixes: rotate it,
  use a bigger hoop, shrink it, or split it into several hoopings.
- **Rotating and mirroring:** `R` and `Shift+R` rotate by 90°, `M`
  mirrors, `C` centers. Saving bakes these in.

## Lettering

Type in the **Text** page and pick a font. The font list shows each font
in its own letters. Bundled fonts come first, grouped into Block; Serif,
slab and varsity; Script; and Rounded and fun. Fonts added from Google
Fonts come next, then the fonts installed on the computer. The number
beside a font, and the line under the list, say about how tall its
capitals must be before thin strokes get too narrow to stitch (1.2 mm).
Below that height the line turns orange.

**Google Fonts** (Machine → Google Fonts, or *Get more fonts from Google
Fonts* at the top of the font list) lists the 1,600 or so free Google
Fonts families with Latin letters:

- Sort them by *Best for stitching* (heavy strokes first), popularity,
  newest or name, and filter by kind (display, handwriting, serif…).
- **Show Letters** draws each family in its own letters. **Show Stitched**
  runs it through the stitch engine with your Lettering fill, edge and
  threads at 15 mm, on your fabric color, so you see how it will sew.
- Type a **sample text** (or *Use my text*) to preview your own words
  instead of the font names.
- Pick a weight and press **Add**. The font goes to
  `~/.local/share/fonts/hoop` with its license, joins the font list and,
  in Lettering, is put to use. Other programs can use it too. **Remove**
  takes it away again.

Hoop needs the internet and `curl` for this. The list is kept for a week.

- **Quick styles** set a whole look in one click:
  - Classic, Satin, Outline, Bold, Vintage, Wavy, Shaded.
  - Key fob, Coaster and Lace, the templates described below.
- **Style page:**
  - Height, letter spacing and slant.
  - **Layout:**
    - Straight or Arc.
    - Circle badge: line 1 round the top, line 2 round the bottom.
    - Arch, Valley, Bulge, Pinch, Bridge, Wave or Grow envelopes.
    - Monogram: three initials, the middle one larger.
  - **Fabric** presets, the **Fill** style and the **Edge** style.
- **Patch page:** a background shape behind the text.
  - Shapes: box, rounded, oval, circle, or fob.
  - Background: filled, light, lace, plain fabric, or appliqué.
  - A satin border.
  - **In the hoop: finish the back** adds a stop for backing fabric
    before the border.
- **Team names** (Text page): one name per line. **Save designs…**
  writes one file per name, in your machine's format, with the current
  settings.

Letters smaller than about 10 mm sew best as **Column** (satin) fill; the
Check list suggests it.

## Artwork

Open an SVG file or a picture (PNG, JPG, WebP), or start from shapes.
Pictures are reduced to a few flat colors and traced first. Logos and
clip art work best; photos do not.

- **Source page:** the size of the design.
  - **Draw:** put a **backdrop picture** behind the hoop and trace it by
    hand with the **Pen**:
    - Click to place a corner.
    - Drag to pull out a curve.
    - Click the first point to close a filled shape.
    - Press `Enter` or double-click to end an open line.
    - Press `Esc` to stop.
- **Tool palette** (left edge of the hoop): the drawing tools, like in a
  drawing program. `Esc` goes back to selecting.
  - **Select** picks and moves shapes; **Edit points** changes the picked
    shape point by point.
  - **Pen** (`B`): as in Draw above.
  - **Rectangle, Ellipse, Polygon, Star** (`U`, press again for the
    next): drag to draw, hold `Shift` for even sides, or click for one
    30 mm across. The strip beside the palette sets round corners, sides,
    star points and inner size for the next one.
  - **Line** (`W`): drag a line. Its ends snap to shape outlines, so it
    can join two shapes; **Bend** in the strip bows it into a curve. An
    end drawn onto a shape stays attached: move or turn the shape and the
    line follows (picked lines show attached ends as green rings).
    **Detach** in the Form card, or editing the line's points, frees it.
  - **Text**: click where the text goes and type. The Shape page's
    **Text** card changes the words, font, height, letter spacing and
    shape (straight, arc, circle badge, arch, wave and more), and the
    letters are redrawn; they stitch like any other shape, so a name
    fits in a frame or on a banner from the symbols. Editing their points
    by hand turns them into ordinary shapes.
  - **Fill bucket** (`K`): click a shape to give it the current thread.
    `Alt+click` takes a shape's thread as the current one.
  - **Stitch angle** and **Start point** of the picked shape, as on the
    Shape page.
  - The **swatch** at the bottom is the current thread: new shapes and
    the bucket use it. Picking a thread there also gives it to the picked
    shapes.
  - Shapes drawn with these tools keep their numbers: pick one and the
    Shape page's **Form** card changes its size, corners, sides, points
    or bend. Editing its points by hand turns it into an ordinary shape.
- **Objects:** every shape, topmost first.
  - Click a shape to pick it, `Ctrl+click` to add another, or drag over
    empty fabric to pick several.
  - Drag shapes on the hoop to move them, use the corner handle to
    resize, and the round knob to turn.
  - Drag a row up or down to restack it, like a layer: higher sews
    later and lies on top. With **Group by thread** on, Hoop otherwise
    sews shapes in thread runs, moving a shape past others it doesn't
    overlap. A shape you drag or arrange (Front, Back, `[`, `]`) gets a
    pin and is sewn exactly where it sits: after everything below it,
    before everything above. Click the pin to let Hoop group it again;
    **Unpin all** releases every shape. The swatch shows the thread the
    shape sews in.
- **Groups:** like folders of layers. Pick shapes and press **Group**
  (`Ctrl+G`, also in Edit); **Ungroup** (`Ctrl+Shift+G`) undoes it.
  - A group shows as a folder row: the arrow folds it, the eye shows or
    hides all of it, a double-click renames it.
  - Click the folder row, or any of its shapes on the hoop, to pick the
    whole group and move or arrange it together. `Alt+click` on the hoop
    picks a single shape inside it.
  - Drag the folder row to move the group as one block. Drag a shape
    into a group (onto its shapes, or just under its folder row) or out
    of it.
  - A group is sewn as one block: nothing from outside is sewn in the
    middle of it. Inside, its shapes still share thread runs as usual.
- **Right-click menu:** right-click a shape on the hoop or a row in
  Objects (it picks it first) for everything you can do to it:
  - Copy, paste, duplicate, delete. `Ctrl+C` and `Ctrl+V` copy shapes
    too, also into another drawing; SVG code on the clipboard still opens
    as artwork.
  - Thread, its stitch look (fill style, outline, appliqué, leave
    fabric), and using its thread for new shapes.
  - Arrange and align; turn 90°, flip either way, reset its move.
  - Group, ungroup, rename the group; pin to its sewing place.
  - Change its form (shapes from the tool palette), edit points, add a
    stitch angle or start point, or clear them.
  - Pick everything in the same thread, or the others instead; hide it,
    hide the others, show everything; Properties opens the Shape page.
  - On empty fabric: paste, select all, add a built-in shape or symbol,
    pick a drawing tool, and the view. In Viewer and Lettering the menu
    rotates, mirrors, centers, pins and plays the design.
- **Optimize…** (top of Objects) tidies the list, with a preview of the
  result before you apply it. By default it names shapes by thread
  ("Black line 3"), merges shapes of one thread and look into one object
  (never across a shape that overlaps them, so the stacking stays as
  drawn), joins lines that touch end to end, and removes specks and
  exact copies. Breaking fills or lines apart (to style one piece on its
  own) is there too, off by default, since it adds objects.
- **Shape page:**
  - Add built-in shapes.
  - **Stitching:**
    - **Add stitch angle:** drag a line across the shape to set the
      direction of its fill rows. Several lines split the fill into
      parts with their own angles.
    - **Set start point:** click where the needle should enter.
  - Arrange, align, recolor, choose a look per shape, transform, and
    edit points.
- **Style page:** with nothing picked, fill and edge styles for the whole
  artwork, plus clean-up (cut hidden parts, drop tiny details). With
  shapes picked, the fill and edge settings apply to just those shapes,
  which then keep their own style (tagged "own style" in Objects). **Use
  the drawing's style** puts them back.
- **Threads page:** one Janome thread per drawing color. Turn colors off,
  give a color a different look (outline, appliqué, a fill style), merge
  colors down to fewer threads.

**Holes and leaving the fabric showing.** A shape on top cuts its area
out of the shapes below it, so nothing is sewn twice. To leave the fabric
itself showing (a white detail on white cloth, a cut-out), give the color
or the shape the look **Leave fabric**: it isn't sewn, and nothing below
is sewn through it. A color you untick in Threads works the same way
(Style → Clean-up → *Colors switched off leave fabric* turns that off).
When a color matches the fabric, Threads offers **Leave fabric?** next
to it.

The default fill, **Auto**, sews narrow parts as satin columns that turn
with the shape and wide parts as tatami.

## Stitch types

- **Fills:**
  - **Auto:** columns where narrow, tatami where wide.
  - **Column:** satin that follows the shape's center lines.
  - **Tatami:** rows of short stitches with staggered needle points.
  - **Satin:** one stitch across per row.
  - **Contour:** rings that follow the outline inward.
  - **Wave** and **Zigzag:** rows that swing.
  - **Hatch:** an open crosshatch.
  - **Gradient:** rows that thin out across the shape.
  - **Open:** sparse rows.
  - **Stipple:** a meandering line that never crosses itself.
  - **Cross-stitch:** an X in every cell.
- **Edges:**
  - **Run** and **Triple** (bean stitch).
  - **Satin.**
  - **Blanket:** an E-stitch, for appliqué.
  - **Scallop.**
- **Underlay** (Style → Fill → More settings): stitches sewn first to
  hold the fabric. Choose auto, edge run, tatami, zigzag, or double
  zigzag for towels and fleece.
- **Fabric presets** set spacing, pull compensation, stitch length and
  underlay together for woven, knit, denim, towel, fleece, felt,
  leather, silk and caps. Hover over one for stabilizer advice.

## Appliqué and in-the-hoop projects

**Appliqué** is set in Lettering → Patch → Appliqué, or in Artwork
through a color's or a shape's look. The machine sews:

1. A **placement line**, then stops. Lay the fabric over the line.
2. A **tack-down**, then stops again. Trim the fabric close to the
   stitching.
3. A **satin cover** that hides the cut edge.

**In-the-hoop** pieces (the Key fob and Coaster quick styles, or the
option on the Patch page) add a stop before the border. Tape backing
fabric under the hoop; a tack-down holds it and the border covers both
edges. Then cut round the border. For a key fob, punch the eyelet open.

**Lace** sews a double lattice that holds together on its own. Hoop two
layers of water-soluble stabilizer and rinse it away afterwards.

## Checking before you sew

The **Check** list (left dock) looks for:

- Thread piling up.
- Needle holes too close together.
- Very short or very long stitches.
- Too many jumps.
- Shapes too thin to fill, or too wide for satin.
- Letters too small.

Click a problem to see where it is. **Fix it this way** applies the
suggested change. `H` shows the thread density on the hoop. Hoop asks
before saving a design that still has problems.

**Test stitch-out** (`Ctrl+T`) has two pages:

- **Test a part:** sew only part of a design first, such as a tight
  corner or the smallest letters, or sew outlines only.
- **Calibration sheet:** samples of satin widths, narrow fills, row
  spacing, pull compensation and small letters. After sewing it, pick
  what looked best. Hoop then uses your results for new designs and
  for the Check list.

## Your machine

Pick your machine in the tool bar's machine and hoop picker, in
**Machine → Machine**, on the Hoop page, or in Settings. It sets:

- **The hoops** you can pick, and which one a new design starts in.
  Hoops marked "Large machines" fit only the bigger models of a brand;
  check your machine's manual.
- **The file format:** Janome JEF, Brother and Baby Lock PES, Pfaff and
  Husqvarna Viking VP3, Bernina EXP, or DST for anything else.
- **Where Send to USB writes:** `EMB/Embf` for Janome, the top of the
  stick for the rest.

## Projects

A **project** (`.hoop`) keeps Artwork or Lettering work so you can carry
on later: the drawing with every edit (shapes drawn, moved or restyled,
groups, attached lines, thread choices), or the text and its settings,
plus the hoop, the machine and any parts pinned to the hoop.

- **File → Save project** (`Ctrl+Alt+S`) saves it; **Save project as…**
  saves a copy under a new name. Open a project like any file, from
  Open, Open recent, the Library (artwork projects show their stitches)
  or by dropping it on the window.
- The title bar shows the project's name, with a dot while there are
  unsaved changes.
- Closing Hoop with unsaved work asks first: save the project, close
  without saving, or cancel.
- Every half minute, unsaved work is also copied aside. If Hoop or the
  computer stops without closing properly, Hoop offers to bring that
  work back the next time it starts.
- A project is for carrying on; the machine still needs its stitch file
  (**Save**, `Ctrl+S`, or Send to USB).

## Saving and sending

- **Save** (`Ctrl+S`): the file in your machine's format.
- **Send to machine** (`Ctrl+U`): writes the file straight onto the
  machine's USB stick. Names of up to 8 letters show best. **Eject**
  closes the stick safely.
- **File → Export as:** the other formats, for another machine.
- **Save worksheet** (`Ctrl+P`): a PDF with the facts, the thread
  sequence, the sewing time, and an actual-size template for placing
  the design on fabric. Print it at 100 %.
- **Save picture** (`Ctrl+Shift+S`): a PNG of the preview.

## Bigger designs and combining

- **Pin to hoop** (`P`): keeps the current design on the hoop so you can
  add the next part, such as a name under a logo. Saving writes all of
  them as one file.
- **File → Split into hoopings:** cuts a design that is too big for the
  hoop into numbered parts.
  - Objects stay whole where they fit.
  - Each part starts with gray registration crosses. After re-hooping,
    line the needle up with them.
  - Pick the crosses out at the end.

## Editing stitch files

For an opened JEF, DST, EXP, PES, PEC or VP3 file:

- **Colors list:** click a swatch to set the real thread color.
  Right-click a thread to sew it earlier or later, or to delete its
  stitches.
- **Design page → Edit stitches:**
  - **Merge repeated threads:** saves thread changes.
  - **Remove tiny stitches:** drops stitches under 0.3 mm.
  - **Pick stitches on the hoop:** click a needle point to pick it.
    Drag it, nudge it with the arrow keys, or press `Del` to remove it.

Every edit can be undone with `Ctrl+Z`.

## Threads

Hoop works in Janome threads inside; a JEF stores their numbers, other
formats store the colors. **Machine → Thread charts** (or Settings) shows another brand's names next
to them: Madeira, Isacord, Robison-Anton, Sulky, Brother and more.

- **Get** downloads a chart from the Ink/Stitch project.
- **My threads:** tick the spools you own. Hoop then suggests those
  first when a color is close enough.

**Copy thread list** in the Colors dock gives a shopping list.

## Settings

**Edit → Settings** (`Ctrl+,`) holds:

- The language: System, English, Polski or Deutsch.
- The designs folder.
- Machine speed, used for sewing times.
- The thread chart.
- The widest satin that Auto sews as columns.
- Display options.
- Your test sheet results.
- The translations folder.
- **Reset all settings**.

Hoop remembers settings in `~/.local/state/hoop/prefs`.

## Demo

**File → Play demo** (or `hoop --demo` from a terminal) plays a 30-second
tour for screen recordings. It types lettering, bends it, makes a badge,
stitches out a library shape, shows the check, opens the wizard, thread
charts, hoopings, settings and manual windows, and flips the languages.
`Esc` stops it. Your own settings come back when it ends.

## Translating Hoop

Every line Hoop shows can be translated with gettext `.po` files:

1. Copy `locales/hoop.pot` to `<language>.po`.
2. Fill it in with a PO editor such as Poedit.
3. Put it in `~/.local/share/hoop/locale/`.
4. Pick the language in Settings.

Lines left empty stay in English.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `N` | New project wizard |
| `O` | Open a file |
| `V` / `L` / `A` | Viewer / Lettering / Artwork |
| `Ctrl+S` | Save in the machine's format |
| `Ctrl+U` | Send to USB |
| `Ctrl+P` | Save worksheet |
| `Ctrl+T` | Test stitch-out |
| `Ctrl+,` | Settings |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `Space` | Play or pause the stitch-out |
| `,` / `.` | One stitch back / forward |
| `[` / `]` | Previous / next thread, or move picked shapes down / up |
| Arrows | Move the design 1 mm (`Shift`: 10 mm) |
| `R` / `Shift+R` | Rotate right / left |
| `M` | Mirror |
| `C` | Center in the hoop |
| `P` | Pin to the hoop |
| `Ctrl+G` / `Ctrl+Shift+G` | Group / ungroup the picked shapes |
| `B` / `U` / `W` / `K` | Artwork: pen / shapes (again for the next) / line / fill bucket |
| `1`–`4` | Pick a hoop |
| `F` / `D` | Whole hoop / zoom to the design |
| `G` / `J` / `T` / `H` | Grid / jumps / thread texture / density |
| `F9` / `F10` | Hide or show the left / right panel |
| `F1` | This manual (Help → User manual) |
| `?` | Shortcut list |
| `Q` | Quit |
