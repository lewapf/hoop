//! Readers for machine embroidery files: Janome JEF, Tajima DST, Melco EXP and
//! Brother PES/PEC. Every reader produces absolute needle positions in 0.1 mm
//! units with y pointing down, which is how the screen draws them.

use std::fmt;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Stitch,
    Jump,
    Trim,
    ColorChange,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Thread {
    pub color: [u8; 3],
    pub name: String,
    pub code: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Jef,
    Dst,
    Exp,
    Pes,
    Pec,
    Vp3,
    Lettering,
    Svg,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Format::Jef => "JEF",
            Format::Dst => "DST",
            Format::Exp => "EXP",
            Format::Pes => "PES",
            Format::Pec => "PEC",
            Format::Vp3 => "VP3",
            Format::Lettering => "Lettering",
            Format::Svg => "SVG artwork",
        };
        f.write_str(s)
    }
}

impl Format {
    /// DST and EXP only record where the machine stops for a new thread, not
    /// which thread, so their colors are placeholders.
    pub fn stores_colors(self) -> bool {
        !matches!(self, Format::Dst | Format::Exp)
    }
}

/// Stitches ready to write: points (0.1 mm), commands, and one thread per
/// color block.
pub type Stitches = (Vec<(f32, f32)>, Vec<Cmd>, Vec<Thread>);

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Pattern {
    pub format: Format,
    pub points: Vec<(f32, f32)>,
    pub cmds: Vec<Cmd>,
    pub threads: Vec<Thread>,
}

#[derive(Debug, PartialEq)]
pub enum ReadError {
    UnknownFormat,
    Truncated,
    Empty,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::UnknownFormat => f.write_str(t!("This isn't a file Hoop can read. It opens JEF, DST, EXP, PES and PEC.")),
            ReadError::Truncated => f.write_str(t!("The file ends early, so it may be damaged.")),
            ReadError::Empty => f.write_str(t!("The file has no stitches in it.")),
        }
    }
}

const fn rgb(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

/// Janome thread chart, indexed by the number stored in the JEF header.
const JEF_THREADS: &[(u32, &str, &str)] = &[
    (0x000000, "Black", "002"),
    (0xffffff, "White", "001"),
    (0xffff17, "Yellow", "204"),
    (0xff6600, "Orange", "203"),
    (0x2f5933, "Olive Green", "219"),
    (0x237336, "Green", "226"),
    (0x65c2c8, "Sky", "217"),
    (0xab5a96, "Purple", "208"),
    (0xf669a0, "Pink", "201"),
    (0xff0000, "Red", "225"),
    (0xb1704e, "Brown", "214"),
    (0x0b2f84, "Blue", "207"),
    (0xe4c35d, "Gold", "003"),
    (0x481a05, "Dark Brown", "205"),
    (0xac9cc7, "Pale Violet", "209"),
    (0xfcf294, "Pale Yellow", "210"),
    (0xf999b7, "Pale Pink", "211"),
    (0xfab381, "Peach", "212"),
    (0xc9a480, "Beige", "213"),
    (0x970533, "Wine Red", "215"),
    (0xa0b8cc, "Pale Sky", "216"),
    (0x7fc21c, "Yellow Green", "218"),
    (0xe5e5e5, "Silver Gray", "220"),
    (0x889b9b, "Gray", "221"),
    (0x98d6bd, "Pale Aqua", "227"),
    (0xb2e1e3, "Baby Blue", "228"),
    (0x368ba0, "Powder Blue", "229"),
    (0x4f83ab, "Bright Blue", "230"),
    (0x386a91, "Slate Blue", "231"),
    (0x071650, "Navy Blue", "232"),
    (0xf99990, "Salmon Pink", "233"),
    (0xf9676b, "Coral", "234"),
    (0xe3311f, "Burnt Orange", "235"),
    (0xe2a188, "Cinnamon", "236"),
    (0xb59474, "Umber", "237"),
    (0xe4cf99, "Blond", "238"),
    (0xffcb00, "Sunflower", "239"),
    (0xe1add4, "Orchid Pink", "240"),
    (0xc3007e, "Peony Purple", "241"),
    (0x80004b, "Burgundy", "242"),
    (0x540571, "Royal Purple", "243"),
    (0xb10525, "Cardinal Red", "244"),
    (0xcae0c0, "Opal Green", "245"),
    (0x899856, "Moss Green", "246"),
    (0x00aa00, "Meadow Green", "247"),
    (0x21613f, "Dark Green", "248"),
    (0x5dae94, "Aquamarine", "249"),
    (0x4cbf8f, "Emerald Green", "250"),
    (0x007372, "Peacock Green", "251"),
    (0x595b61, "Dark Gray", "252"),
    (0xfffff2, "Ivory White", "253"),
    (0xb15818, "Hazel", "254"),
    (0xcb8a07, "Toast", "255"),
];

/// Brother thread chart, indexed by the number stored in the PEC header.
const PEC_THREADS: &[(u32, &str)] = &[
    (0x1a0a94, "Prussian Blue"),
    (0x0f75ff, "Blue"),
    (0x00934c, "Teal Green"),
    (0xbabdfe, "Cornflower Blue"),
    (0xec0000, "Red"),
    (0xe4995a, "Reddish Brown"),
    (0xcc48ab, "Magenta"),
    (0xfdc4fa, "Light Lilac"),
    (0xdd84cd, "Lilac"),
    (0x6bd38a, "Mint Green"),
    (0xe4a945, "Deep Gold"),
    (0xffbd42, "Orange"),
    (0xffe600, "Yellow"),
    (0x6cd900, "Lime Green"),
    (0xc1a941, "Brass"),
    (0xb5ad97, "Silver"),
    (0xba9c5f, "Russet Brown"),
    (0xfaf59e, "Cream Brown"),
    (0x808080, "Pewter"),
    (0x000000, "Black"),
    (0x001cdf, "Ultramarine"),
    (0xdf00b8, "Royal Purple"),
    (0x626262, "Dark Gray"),
    (0x69260d, "Dark Brown"),
    (0xff0060, "Deep Rose"),
    (0xbf8200, "Light Brown"),
    (0xf39178, "Salmon Pink"),
    (0xff6805, "Vermilion"),
    (0xf0f0f0, "White"),
    (0xc832cd, "Violet"),
    (0xb0bf9b, "Seacrest"),
    (0x65bfeb, "Sky Blue"),
    (0xffba04, "Pumpkin"),
    (0xfff06c, "Cream Yellow"),
    (0xfeca15, "Khaki"),
    (0xf38101, "Clay Brown"),
    (0x37a923, "Leaf Green"),
    (0x23465f, "Peacock Blue"),
    (0xa6a6a6, "Gray"),
    (0xcebfa6, "Warm Gray"),
    (0x96aa02, "Dark Olive"),
    (0xffe3c6, "Linen"),
    (0xff99d7, "Pink"),
];

const STOCK: [u32; 12] = [0x1f4fbf, 0xd4202a, 0x1d8a3c, 0xf2b705, 0x7b2fa8, 0xf07c1a, 0x111111, 0x1aa6b7, 0xe05a9a, 0x6b4a2b, 0x8fbf2a, 0x5a5a5a];

pub fn stock_thread(i: usize) -> Thread {
    Thread { color: rgb(STOCK[i % STOCK.len()]), name: format!("Thread {}", i + 1), code: String::new() }
}

pub fn jef_thread(index: i32, i: usize) -> Thread {
    match JEF_THREADS.get((index.unsigned_abs() as usize).wrapping_sub(1)) {
        Some(&(c, name, code)) => Thread { color: rgb(c), name: name.into(), code: code.into() },
        None => stock_thread(i),
    }
}

fn pec_thread(index: u8, i: usize) -> Thread {
    match PEC_THREADS.get((index as usize).wrapping_sub(1)) {
        Some(&(c, name)) => Thread { color: rgb(c), name: name.into(), code: index.to_string() },
        None => stock_thread(i),
    }
}

/// Accumulates relative moves into absolute positions.
#[derive(Default)]
struct Builder {
    x: f32,
    y: f32,
    points: Vec<(f32, f32)>,
    cmds: Vec<Cmd>,
}

impl Builder {
    fn add(&mut self, dx: i32, dy: i32, cmd: Cmd) {
        self.x += dx as f32;
        self.y += dy as f32;
        self.points.push((self.x, self.y));
        self.cmds.push(cmd);
    }

    fn finish(self, format: Format, threads: Vec<Thread>) -> Pattern {
        let mut pattern = Pattern { format, points: self.points, cmds: self.cmds, threads };
        let stops = pattern.cmds.iter().filter(|c| **c == Cmd::ColorChange).count() + 1;
        while pattern.threads.len() < stops {
            pattern.threads.push(stock_thread(pattern.threads.len()));
        }
        pattern
    }
}

fn i32_le(d: &[u8], at: usize) -> Result<i32, ReadError> {
    d.get(at..at + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or(ReadError::Truncated)
}

fn s8(b: u8) -> i32 {
    b as i8 as i32
}

fn read_jef(d: &[u8]) -> Result<Pattern, ReadError> {
    let offset = i32_le(d, 0)? as usize;
    let colors = i32_le(d, 24)?.max(0) as usize;
    let threads =
        (0..colors).take_while(|i| 116 + i * 4 + 4 <= offset).map(|i| i32_le(d, 116 + i * 4).map(|idx| jef_thread(idx, i))).collect::<Result<Vec<_>, _>>()?;
    let mut b = Builder::default();
    let mut p = offset;
    while p + 1 < d.len() {
        let (b0, b1) = (d[p], d[p + 1]);
        p += 2;
        if b0 != 0x80 {
            b.add(s8(b0), -s8(b1), Cmd::Stitch);
            continue;
        }
        if b1 == 0x10 || p + 1 >= d.len() {
            break;
        }
        let (dx, dy) = (s8(d[p]), -s8(d[p + 1]));
        p += 2;
        b.add(dx, dy, if b1 == 0x01 { Cmd::ColorChange } else { Cmd::Jump });
    }
    Ok(b.finish(Format::Jef, threads))
}

fn read_dst(d: &[u8]) -> Result<Pattern, ReadError> {
    let bit = |v: u8, n: u32| ((v >> n) & 1) as i32;
    let mut b = Builder::default();
    for rec in d.get(512..).unwrap_or(&[]).chunks_exact(3) {
        let (b0, b1, b2) = (rec[0], rec[1], rec[2]);
        if b2 & 0xf3 == 0xf3 {
            break;
        }
        let dx = 81 * (bit(b2, 2) - bit(b2, 3))
            + 27 * (bit(b1, 2) - bit(b1, 3))
            + 9 * (bit(b0, 2) - bit(b0, 3))
            + 3 * (bit(b1, 0) - bit(b1, 1))
            + (bit(b0, 0) - bit(b0, 1));
        let dy = 81 * (bit(b2, 5) - bit(b2, 4))
            + 27 * (bit(b1, 5) - bit(b1, 4))
            + 9 * (bit(b0, 5) - bit(b0, 4))
            + 3 * (bit(b1, 7) - bit(b1, 6))
            + (bit(b0, 7) - bit(b0, 6));
        let cmd = if b2 & 0xc3 == 0xc3 {
            Cmd::ColorChange
        } else if b2 & 0x80 != 0 {
            Cmd::Jump
        } else {
            Cmd::Stitch
        };
        b.add(dx, -dy, cmd);
    }
    Ok(b.finish(Format::Dst, Vec::new()))
}

fn read_exp(d: &[u8]) -> Result<Pattern, ReadError> {
    let mut b = Builder::default();
    let mut p = 0;
    while p + 1 < d.len() {
        let (b0, b1) = (d[p], d[p + 1]);
        p += 2;
        if b0 != 0x80 {
            b.add(s8(b0), -s8(b1), Cmd::Stitch);
            continue;
        }
        if p + 1 >= d.len() {
            break;
        }
        let (dx, dy) = (s8(d[p]), -s8(d[p + 1]));
        p += 2;
        match b1 {
            _ if b1 & 0x01 != 0 => b.add(dx, dy, Cmd::ColorChange),
            0x80 => b.add(0, 0, Cmd::Trim),
            0x04 => b.add(dx, dy, Cmd::Jump),
            _ => b.add(dx, dy, Cmd::Stitch),
        }
    }
    Ok(b.finish(Format::Exp, Vec::new()))
}

/// Pfaff and Husqvarna Viking VP3: big-endian blocks, each tagged and
/// sized, with one block per thread holding its color and its stitches.
/// Positions are in 1/100 of the stitch unit, y up; stitches in 0.1 mm.
fn read_vp3(d: &[u8]) -> Result<Pattern, ReadError> {
    struct R<'a> {
        d: &'a [u8],
        p: usize,
    }
    impl R<'_> {
        fn take(&mut self, n: usize) -> Result<&[u8], ReadError> {
            let b = self.d.get(self.p..self.p + n).ok_or(ReadError::Truncated)?;
            self.p += n;
            Ok(b)
        }
        fn skip(&mut self, n: usize) -> Result<(), ReadError> {
            self.take(n).map(|_| ())
        }
        fn u16(&mut self) -> Result<usize, ReadError> {
            self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
        }
        fn i32(&mut self) -> Result<i32, ReadError> {
            self.take(4).map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        }
        fn string(&mut self) -> Result<String, ReadError> {
            let n = self.u16()?;
            Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
        }
    }
    let mut r = R { d, p: 6 };
    // Maker, the file block's tag and size, a comment.
    r.string()?;
    r.skip(7)?;
    r.string()?;
    // Extents, stitch count, color count and flags, the design block's tag and size.
    r.skip(32)?;
    let cx = r.i32()? as f32 / 100.0;
    let cy = -(r.i32()? as f32) / 100.0;
    r.skip(27)?;
    r.string()?;
    r.skip(24)?;
    r.string()?;
    let colors = r.u16()?;
    let mut points = Vec::new();
    let mut cmds = Vec::new();
    let mut threads = Vec::new();
    for k in 0..colors {
        r.skip(3)?;
        let end = r.i32()?.max(0) as usize + r.p;
        let sx = r.i32()? as f32 / 100.0 + cx;
        let sy = -(r.i32()? as f32) / 100.0 + cy;
        // The thread: one or more colors (the first is the one shown).
        let shades = r.take(2)?[0].max(1) as usize;
        let mut color = [0u8; 3];
        for m in 0..shades {
            let c = r.take(3)?;
            if m == 0 {
                color = [c[0], c[1], c[2]];
            }
            r.skip(3)?;
        }
        r.skip(2)?;
        let code = r.string()?;
        let name = r.string()?;
        r.string()?;
        threads.push(Thread { color, name, code });
        // Its end offset from the start, then the stitch block's tag, size and a fixed 3 bytes.
        r.skip(8 + 7 + 3)?;
        if k > 0
            && let Some(&last) = points.last()
        {
            points.push(last);
            cmds.push(Cmd::ColorChange);
        }
        // Move to where this thread starts.
        let (mut x, mut y) = (sx, sy);
        if points.last() != Some(&(x, y)) {
            points.push((x, y));
            cmds.push(Cmd::Jump);
        }
        let stitches = d.get(r.p..end.min(d.len())).ok_or(ReadError::Truncated)?;
        let mut i = 0;
        let mut jump = false;
        while i + 1 < stitches.len() {
            let (a, b) = (stitches[i], stitches[i + 1]);
            i += 2;
            let (dx, dy) = if a != 0x80 {
                (s8(a), s8(b))
            } else {
                match b {
                    0x01 => {
                        let Some(v) = stitches.get(i..i + 4) else { break };
                        i += 4;
                        (i16::from_be_bytes([v[0], v[1]]) as i32, i16::from_be_bytes([v[2], v[3]]) as i32)
                    }
                    0x03 => {
                        // A cut: the next move is a jump.
                        jump = true;
                        continue;
                    }
                    _ => continue,
                }
            };
            x += dx as f32;
            y += dy as f32;
            points.push((x, y));
            cmds.push(if std::mem::take(&mut jump) { Cmd::Jump } else { Cmd::Stitch });
        }
        r.p = end;
    }
    let b = Builder { x: 0.0, y: 0.0, points, cmds };
    Ok(b.finish(Format::Vp3, threads))
}

/// Returns the stitches and whether the end marker was reached, which tells a
/// correct start offset from a wrong one.
fn read_pec_stitches(d: &[u8], start: usize) -> (Builder, bool) {
    let mut b = Builder::default();
    let mut p = start;
    let byte = |p: &mut usize| -> Option<u8> {
        let v = d.get(*p).copied();
        *p += 1;
        v
    };
    loop {
        let (Some(v1), Some(mut v2)) = (byte(&mut p), byte(&mut p)) else { return (b, false) };
        if v1 == 0xff && v2 == 0x00 {
            return (b, true);
        }
        if v1 == 0xfe && v2 == 0xb0 {
            p += 1;
            b.add(0, 0, Cmd::ColorChange);
            continue;
        }
        let mut cmd = Cmd::Stitch;
        let dx = if v1 & 0x80 != 0 {
            if v1 & 0x30 != 0 {
                cmd = Cmd::Jump;
            }
            let x = ((v1 as i32 & 0x0f) << 8) | v2 as i32;
            let Some(next) = byte(&mut p) else { return (b, false) };
            v2 = next;
            if x & 0x800 != 0 { x - 0x1000 } else { x }
        } else if v1 > 0x3f {
            v1 as i32 - 0x80
        } else {
            v1 as i32
        };
        let dy = if v2 & 0x80 != 0 {
            if v2 & 0x30 != 0 {
                cmd = Cmd::Jump;
            }
            let Some(lo) = byte(&mut p) else { return (b, false) };
            let y = ((v2 as i32 & 0x0f) << 8) | lo as i32;
            if y & 0x800 != 0 { y - 0x1000 } else { y }
        } else if v2 > 0x3f {
            v2 as i32 - 0x80
        } else {
            v2 as i32
        };
        // A PEC trim still moves the needle, so it is drawn like a jump.
        b.add(dx, dy, cmd);
    }
}

fn read_pec(d: &[u8], pec: usize, format: Format) -> Result<Pattern, ReadError> {
    let count = *d.get(pec + 48).ok_or(ReadError::Truncated)? as usize + 1;
    let threads = (0..count).map(|i| d.get(pec + 49 + i).map(|&idx| pec_thread(idx, i)).ok_or(ReadError::Truncated)).collect::<Result<Vec<_>, _>>()?;
    // Most writers start the stitches 532 bytes into the PEC block; a few use 528.
    let (mut b, clean) = read_pec_stitches(d, pec + 532);
    if !clean {
        let (alt, alt_clean) = read_pec_stitches(d, pec + 528);
        if alt_clean {
            b = alt;
        }
    }
    Ok(b.finish(format, threads))
}

pub fn detect(name: &str, d: &[u8]) -> Option<Format> {
    if d.starts_with(b"#PES") {
        return Some(Format::Pes);
    }
    if d.starts_with(b"#PEC") {
        return Some(Format::Pec);
    }
    if d.starts_with(b"%vsm%") {
        return Some(Format::Vp3);
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jef" => Some(Format::Jef),
        "dst" => Some(Format::Dst),
        "exp" => Some(Format::Exp),
        "pes" => Some(Format::Pes),
        "pec" => Some(Format::Pec),
        "vp3" => Some(Format::Vp3),
        _ if d.starts_with(b"LA:") => Some(Format::Dst),
        _ => None,
    }
}

pub fn read(name: &str, d: &[u8]) -> Result<Pattern, ReadError> {
    let pattern = match detect(name, d).ok_or(ReadError::UnknownFormat)? {
        Format::Jef => read_jef(d)?,
        Format::Dst => read_dst(d)?,
        Format::Exp => read_exp(d)?,
        Format::Pes => read_pec(d, i32_le(d, 8)?.max(0) as usize, Format::Pes)?,
        Format::Pec => read_pec(d, 8, Format::Pec)?,
        Format::Vp3 => read_vp3(d)?,
        Format::Lettering | Format::Svg => return Err(ReadError::UnknownFormat),
    };
    if !pattern.cmds.contains(&Cmd::Stitch) {
        return Err(ReadError::Empty);
    }
    Ok(pattern)
}

/// One run of a single thread, from one color change to the next.
#[derive(Clone, Debug)]
pub struct Block {
    pub start: usize,
    pub end: usize,
    pub thread: Thread,
    pub stitches: usize,
}

impl Pattern {
    /// Bounds of the actual stitches as (min x, min y, max x, max y).
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let mut r = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (&(x, y), _) in self.points.iter().zip(&self.cmds).filter(|(_, c)| **c == Cmd::Stitch) {
            r = (r.0.min(x), r.1.min(y), r.2.max(x), r.3.max(y));
        }
        r
    }

    pub fn blocks(&self) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut start = 0;
        for i in 1..=self.cmds.len() {
            if i == self.cmds.len() || self.cmds[i] == Cmd::ColorChange {
                let k = blocks.len();
                let stitches = self.cmds[start..i].iter().filter(|c| **c == Cmd::Stitch).count();
                let thread = self.threads.get(k).cloned().unwrap_or_else(|| stock_thread(k));
                blocks.push(Block { start, end: i, thread, stitches });
                start = i;
            }
        }
        // A trailing color change with nothing after it would show as an empty thread.
        blocks.retain(|b| b.stitches > 0);
        blocks
    }

    /// Approximate top thread used by a block, in meters. Thread runs a bit
    /// longer than the stitches because it goes down into the fabric.
    pub fn thread_meters(&self, b: &Block) -> f32 {
        let mut sum = 0.0;
        for i in b.start.max(1)..b.end {
            if self.cmds[i] == Cmd::Stitch {
                let (p, q) = (self.points[i - 1], self.points[i]);
                sum += ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
            }
        }
        sum / 10_000.0 * 1.2
    }

    /// Things likely to stitch badly, as short sentences.
    pub fn warnings(&self) -> Vec<String> {
        let mut long = 0;
        let mut tiny = 0;
        let mut stitches = 0;
        for i in 1..self.cmds.len() {
            if self.cmds[i] != Cmd::Stitch || self.cmds[i - 1] == Cmd::ColorChange {
                continue;
            }
            stitches += 1;
            let (p, q) = (self.points[i - 1], self.points[i]);
            let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
            if d > 80.0 {
                long += 1;
            } else if d > 0.0 && d < 3.0 {
                tiny += 1;
            }
        }
        let mut out = Vec::new();
        if long > 0 {
            out.push(format!("{long} stitches are longer than 8 mm and may snag. Use tatami or a narrower satin."));
        }
        if tiny * 50 > stitches.max(1) {
            out.push(format!("{tiny} stitches are under 0.3 mm, which can break the thread. Make small details bigger."));
        }
        let jumps = self.count(Cmd::Jump);
        if jumps > 60 {
            out.push(format!("{jumps} jumps mean a lot of trimming. Fewer, larger shapes stitch faster."));
        }
        out
    }

    pub fn count(&self, cmd: Cmd) -> usize {
        self.cmds.iter().filter(|c| **c == cmd).count()
    }
}

/// The Janome chart as (thread number, thread), for picking colors.
pub fn jef_palette() -> impl Iterator<Item = (i32, Thread)> {
    (1..=JEF_THREADS.len() as i32).map(|i| (i, jef_thread(i, 0)))
}

/// The Janome thread closest to a color, for files whose threads aren't Janome's.
pub fn nearest_jef(c: [u8; 3]) -> i32 {
    let d = |t: u32| {
        let [r, g, b] = rgb(t);
        let (dr, dg, db) = (r as i32 - c[0] as i32, g as i32 - c[1] as i32, b as i32 - c[2] as i32);
        2 * dr * dr + 4 * dg * dg + 3 * db * db
    };
    (0..JEF_THREADS.len()).min_by_key(|&i| d(JEF_THREADS[i].0)).map_or(1, |i| i as i32 + 1)
}

/// The Brother thread (1-based PEC index) closest to a color.
pub fn nearest_pec(c: [u8; 3]) -> u8 {
    let d = |t: u32| {
        let [r, g, b] = rgb(t);
        let (dr, dg, db) = (r as i32 - c[0] as i32, g as i32 - c[1] as i32, b as i32 - c[2] as i32);
        2 * dr * dr + 4 * dg * dg + 3 * db * db
    };
    (0..PEC_THREADS.len()).min_by_key(|&i| d(PEC_THREADS[i].0)).map_or(20, |i| i as u8 + 1)
}

/// Today's date as "YYYY-MM-DD" (UTC).
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Writes a Janome JEF file. Positions are in 0.1 mm with y down and the
/// design centered on the origin; `threads` are Janome thread numbers, one per
/// color block.
pub fn write_jef(points: &[(f32, f32)], cmds: &[Cmd], threads: &[i32]) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();
    let (mut x, mut y) = (0i32, 0i32);
    let (mut minx, mut miny, mut maxx, mut maxy) = (0i32, 0i32, 0i32, 0i32);
    let mut first = true;
    for (&(px, py), &cmd) in points.iter().zip(cmds) {
        let (tx, ty) = (px.round() as i32, py.round() as i32);
        if cmd == Cmd::ColorChange {
            data.extend_from_slice(&[0x80, 0x01, 0, 0]);
        }
        // Moves longer than 12.7 mm don't fit in one record, so split them.
        let (dx, dy) = (tx - x, ty - y);
        let steps = ((dx.abs().max(dy.abs()) + 126) / 127).max(1);
        let jump = first || cmd != Cmd::Stitch;
        for k in 1..=steps {
            let (nx, ny) = (x + dx * k / steps - x - dx * (k - 1) / steps, y + dy * k / steps - y - dy * (k - 1) / steps);
            if jump {
                data.extend_from_slice(&[0x80, 0x02]);
            }
            data.extend_from_slice(&[nx as i8 as u8, (-ny) as i8 as u8]);
        }
        x = tx;
        y = ty;
        first = false;
        if cmd == Cmd::Stitch {
            (minx, miny, maxx, maxy) = (minx.min(x), miny.min(y), maxx.max(x), maxy.max(y));
        }
    }
    data.extend_from_slice(&[0x80, 0x10]);

    let (left, top, right, bottom) = (-minx, -miny, maxx, maxy);
    let fits = |w: i32, h: i32| left <= w && right <= w && top <= h && bottom <= h;
    let hoop = if fits(550, 550) {
        0
    } else if fits(630, 550) {
        3
    } else if fits(700, 1000) {
        2
    } else {
        4
    };
    let n = threads.len().max(1);
    let offset = 116 + 8 * n;
    let mut h: Vec<u8> = Vec::with_capacity(offset + data.len());
    let int = |h: &mut Vec<u8>, v: i32| h.extend_from_slice(&v.to_le_bytes());
    int(&mut h, offset as i32);
    int(&mut h, 0x14);
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (yr, mo, dd) = civil(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    h.extend_from_slice(format!("{yr:04}{mo:02}{dd:02}{:02}{:02}{:02}", t / 3600, t / 60 % 60, t % 60).as_bytes());
    h.extend_from_slice(&[0, 0]);
    int(&mut h, n as i32);
    int(&mut h, (data.len() / 2) as i32);
    int(&mut h, hoop);
    for v in [left, top, right, bottom] {
        int(&mut h, v);
    }
    // Distance from the design to each edge of Janome's four reference hoops.
    for (hw, hh) in [(550, 550), (250, 250), (700, 1000), (630, 550)] {
        let e = [hw - left, hh - top, hw - right, hh - bottom];
        let ok = e.iter().all(|v| *v >= 0);
        for v in e {
            int(&mut h, if ok { v } else { -1 });
        }
    }
    for i in 0..n {
        int(&mut h, threads.get(i).copied().unwrap_or(1));
    }
    for _ in 0..n {
        int(&mut h, 0x0d);
    }
    h.extend_from_slice(&data);
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEART: &[u8] = include_bytes!("../tests/fixtures/heart_5x5_fill.jef");
    const GEAR: &[u8] = include_bytes!("../tests/fixtures/GEARBRMN.dst");

    fn size_mm(p: &Pattern) -> (f32, f32) {
        let b = p.bounds();
        ((b.2 - b.0) / 10.0, (b.3 - b.1) / 10.0)
    }

    #[test]
    fn reads_jef_size_and_thread() {
        let p = read("heart.jef", HEART).unwrap();
        assert_eq!(p.format, Format::Jef);
        assert_eq!(p.count(Cmd::Stitch), 9311);
        let (w, h) = size_mm(&p);
        assert!((w - 127.2).abs() < 0.2 && (h - 127.4).abs() < 0.2, "{w} x {h}");
        assert_eq!(p.threads[0].name, "Cardinal Red");
    }

    #[test]
    fn reads_dst_with_placeholder_colors() {
        let p = read("GEARBRMN.DST", GEAR).unwrap();
        assert_eq!(p.format, Format::Dst);
        assert_eq!(p.count(Cmd::Stitch), 7265);
        let (w, h) = size_mm(&p);
        assert!((w - 130.9).abs() < 0.2 && (h - 51.5).abs() < 0.2, "{w} x {h}");
        assert_eq!(p.blocks().len(), 1);
    }

    #[test]
    fn reads_jef_color_changes_into_blocks() {
        // Header: stitch offset 124, two colors (black, red), then stitches.
        let mut d = vec![0u8; 124];
        d[0..4].copy_from_slice(&124i32.to_le_bytes());
        d[24..28].copy_from_slice(&2i32.to_le_bytes());
        d[116..120].copy_from_slice(&1i32.to_le_bytes());
        d[120..124].copy_from_slice(&10i32.to_le_bytes());
        d.extend_from_slice(&[10, 0, 10, 0, 0x80, 0x01, 0, 0, 0, 10, 0x80, 0x10]);
        let p = read("x.jef", &d).unwrap();
        let blocks = p.blocks();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].thread.name, "Black");
        assert_eq!(blocks[1].thread.name, "Red");
        assert_eq!(p.points.last(), Some(&(20.0, -10.0)));
    }

    #[test]
    fn reads_pec_long_and_short_moves() {
        let mut d = b"#PEC0001".to_vec();
        d.resize(8 + 532, 0);
        d[8 + 48] = 0; // one color
        d[8 + 49] = 5; // Red
        // short +5,-3; long jump +300,0; color change; end
        d.extend_from_slice(&[5, 0x7d, 0x91, 0x2c, 0x00, 0xfe, 0xb0, 0x01, 1, 1, 0xff, 0x00]);
        let p = read("x.pec", &d).unwrap();
        assert_eq!(p.points[0], (5.0, -3.0));
        assert_eq!(p.points[1], (305.0, -3.0));
        assert_eq!(p.cmds[1], Cmd::Jump);
        assert_eq!(p.threads[0].name, "Red");
    }

    #[test]
    fn jef_round_trips_through_writer() {
        let p = read("heart.jef", HEART).unwrap();
        let b = p.bounds();
        let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
        let pts: Vec<(f32, f32)> = p.points.iter().map(|&(x, y)| (x - cx, y - cy)).collect();
        let out = write_jef(&pts, &p.cmds, &[42]);
        let q = read("out.jef", &out).unwrap();
        assert_eq!(q.count(Cmd::Stitch), p.count(Cmd::Stitch));
        assert_eq!(q.threads[0].name, "Cardinal Red");
        assert_eq!(q.bounds().2 - q.bounds().0, b.2 - b.0);
        // Same header layout as the machine-tested original: point count and hoop code.
        assert_eq!(i32_le(&out, 28).unwrap(), i32_le(HEART, 28).unwrap());
        assert_eq!(i32_le(&out, 32).unwrap(), i32_le(HEART, 32).unwrap());
    }

    #[test]
    fn long_moves_are_split() {
        let out = write_jef(&[(0.0, 0.0), (300.0, 0.0)], &[Cmd::Jump, Cmd::Stitch], &[1]);
        let q = read("x.jef", &out).unwrap();
        assert_eq!(q.points.last(), Some(&(300.0, 0.0)));
        assert_eq!(nearest_jef([0xb0, 0x05, 0x26]), 42);
    }

    #[test]
    fn rejects_unknown_and_empty_files() {
        assert_eq!(read("notes.txt", b"hello").unwrap_err(), ReadError::UnknownFormat);
        assert_eq!(read("x.exp", &[]).unwrap_err(), ReadError::Empty);
    }
}
