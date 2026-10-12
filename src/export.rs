//! Writers for the formats other machines read: Tajima DST, Melco EXP,
//! Brother PES/PEC and Pfaff/Husqvarna VP3. JEF lives in `formats.rs`.
//! Input is the same as for `write_jef`: absolute points in 0.1 mm, y down,
//! the design centered on the origin, one thread per color block.

use crate::formats::{self, Cmd, Thread};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Jef,
    Dst,
    Exp,
    Pes,
    Pec,
    Vp3,
}

impl Out {
    pub const ALL: [Out; 6] = [Out::Jef, Out::Dst, Out::Exp, Out::Pes, Out::Pec, Out::Vp3];

    pub fn ext(self) -> &'static str {
        match self {
            Out::Jef => "jef",
            Out::Dst => "dst",
            Out::Exp => "exp",
            Out::Pes => "pes",
            Out::Pec => "pec",
            Out::Vp3 => "vp3",
        }
    }

    /// Menu text: the format and the machines that read it.
    pub fn label(self) -> &'static str {
        match self {
            Out::Jef => t!("JEF · Janome"),
            Out::Dst => t!("DST · Tajima, most machines"),
            Out::Exp => t!("EXP · Melco, Bernina"),
            Out::Pes => t!("PES · Brother, Baby Lock"),
            Out::Pec => t!("PEC · Brother"),
            Out::Vp3 => t!("VP3 · Pfaff, Husqvarna Viking"),
        }
    }

    pub fn filter(self) -> &'static str {
        match self {
            Out::Jef => t!("Janome JEF"),
            Out::Dst => t!("Tajima DST"),
            Out::Exp => t!("Melco EXP"),
            Out::Pes => t!("Brother PES"),
            Out::Pec => t!("Brother PEC"),
            Out::Vp3 => t!("Pfaff VP3"),
        }
    }
}

/// Writes the stitches in `out`'s format. `name` goes into headers that
/// carry a label.
pub fn write(out: Out, name: &str, points: &[(f32, f32)], cmds: &[Cmd], threads: &[Thread]) -> Vec<u8> {
    match out {
        Out::Jef => formats::write_jef(points, cmds, &jef_numbers(threads)),
        Out::Dst => write_dst(name, points, cmds),
        Out::Exp => write_exp(points, cmds),
        Out::Pes => {
            // A short PES header pointing straight at the PEC part, which is
            // all the machines read.
            let mut d = b"#PES0001".to_vec();
            d.extend_from_slice(&22u32.to_le_bytes());
            d.extend_from_slice(&[0; 10]);
            d.extend(pec_block(name, points, cmds, threads));
            d
        }
        Out::Pec => {
            let mut d = b"#PEC0001".to_vec();
            d.extend(pec_block(name, points, cmds, threads));
            d
        }
        Out::Vp3 => write_vp3(points, cmds, threads),
    }
}

/// Janome thread numbers for threads: exact chart matches keep their number,
/// other colors take the nearest Janome thread.
pub fn jef_numbers(threads: &[Thread]) -> Vec<i32> {
    let palette: Vec<(i32, Thread)> = formats::jef_palette().collect();
    threads.iter().map(|t| palette.iter().find(|(_, p)| p == t).map_or_else(|| formats::nearest_jef(t.color), |(i, _)| *i)).collect()
}

/// Integer moves between the points, each split into steps no longer than
/// `max` on either axis. Yields (dx, dy, cmd, first step of this move).
fn moves(points: &[(f32, f32)], cmds: &[Cmd], max: i32) -> Vec<(i32, i32, Cmd, bool)> {
    let mut out = Vec::new();
    let (mut x, mut y) = (0i32, 0i32);
    for (&(px, py), &cmd) in points.iter().zip(cmds) {
        let (tx, ty) = (px.round() as i32, py.round() as i32);
        let (dx, dy) = (tx - x, ty - y);
        let steps = ((dx.abs().max(dy.abs()) + max - 1) / max).max(1);
        for k in 1..=steps {
            let sx = dx * k / steps - dx * (k - 1) / steps;
            let sy = dy * k / steps - dy * (k - 1) / steps;
            // A split stitch moves as jumps and stitches at the end.
            let c = if cmd == Cmd::Stitch && k < steps { Cmd::Jump } else { cmd };
            out.push((sx, sy, c, k == 1));
        }
        (x, y) = (tx, ty);
    }
    out
}

fn stitch_bounds(points: &[(f32, f32)], cmds: &[Cmd]) -> (i32, i32, i32, i32) {
    let mut b = (0, 0, 0, 0);
    for (&(x, y), _) in points.iter().zip(cmds).filter(|(_, c)| **c == Cmd::Stitch) {
        let (x, y) = (x.round() as i32, y.round() as i32);
        b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    }
    b
}

fn dst_record(dx: i32, dy: i32, jump: bool) -> [u8; 3] {
    // Balanced ternary: each axis is a sum of ±81, ±27, ±9, ±3, ±1.
    // DST's y points up.
    let (mut x, mut y) = (dx, -dy);
    let mut b = [0u8, 0u8, 0x03];
    if jump {
        b[2] |= 0x80;
    }
    for (w, byte, plus, minus) in [(81, 2, 2, 3), (27, 1, 2, 3), (9, 0, 2, 3), (3, 1, 0, 1), (1, 0, 0, 1)] {
        let half = w / 2;
        if x > half {
            b[byte] |= 1 << plus;
            x -= w;
        } else if x < -half {
            b[byte] |= 1 << minus;
            x += w;
        }
    }
    for (w, byte, plus, minus) in [(81, 2, 5, 4), (27, 1, 5, 4), (9, 0, 5, 4), (3, 1, 7, 6), (1, 0, 7, 6)] {
        let half = w / 2;
        if y > half {
            b[byte] |= 1 << plus;
            y -= w;
        } else if y < -half {
            b[byte] |= 1 << minus;
            y += w;
        }
    }
    b
}

fn write_dst(name: &str, points: &[(f32, f32)], cmds: &[Cmd]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut count = 0;
    for (dx, dy, cmd, first) in moves(points, cmds, 121) {
        if cmd == Cmd::ColorChange && first {
            data.extend_from_slice(&[0, 0, 0xc3]);
        }
        if cmd == Cmd::Trim {
            continue;
        }
        data.extend_from_slice(&dst_record(dx, dy, cmd != Cmd::Stitch));
        count += 1;
    }
    data.extend_from_slice(&[0, 0, 0xf3]);
    let (minx, miny, maxx, maxy) = stitch_bounds(points, cmds);
    let colors = cmds.iter().filter(|c| **c == Cmd::ColorChange).count();
    let (ax, ay) = points.last().map_or((0, 0), |p| (p.0.round() as i32, -(p.1.round() as i32)));
    let label: String = name.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(16).collect();
    let sign = |v: i32| if v < 0 { '-' } else { '+' };
    let mut h = format!(
        "LA:{label:<16}\rST:{count:>7}\rCO:{colors:>3}\r+X:{:>5}\r-X:{:>5}\r+Y:{:>5}\r-Y:{:>5}\rAX:{}{:>5}\rAY:{}{:>5}\rMX:+{:>5}\rMY:+{:>5}\rPD:{:>6}\r\x1a",
        maxx,
        -minx,
        -miny,
        maxy,
        sign(ax),
        ax.abs(),
        sign(ay),
        ay.abs(),
        0,
        0,
        "******"
    )
    .into_bytes();
    h.resize(512, b' ');
    h.extend(data);
    h
}

fn write_exp(points: &[(f32, f32)], cmds: &[Cmd]) -> Vec<u8> {
    let mut d = Vec::new();
    let mut jumping = false;
    for (dx, dy, cmd, first) in moves(points, cmds, 127) {
        let (bx, by) = (dx as i8 as u8, (-dy) as i8 as u8);
        match cmd {
            Cmd::Stitch => {
                jumping = false;
                d.extend_from_slice(&[bx, by]);
            }
            Cmd::Jump => {
                // Cut the thread before travelling.
                if !jumping && !d.is_empty() {
                    d.extend_from_slice(&[0x80, 0x80, 0x07, 0x00]);
                }
                jumping = true;
                d.extend_from_slice(&[0x80, 0x04, bx, by]);
            }
            Cmd::ColorChange => {
                if first {
                    d.extend_from_slice(&[0x80, 0x01, 0, 0]);
                }
                if dx != 0 || dy != 0 {
                    d.extend_from_slice(&[0x80, 0x04, bx, by]);
                }
            }
            Cmd::Trim => d.extend_from_slice(&[0x80, 0x80, 0x07, 0x00]),
        }
    }
    d
}

/// Brother's thread index (1-based) nearest to each thread.
fn pec_numbers(threads: &[Thread]) -> Vec<u8> {
    threads.iter().map(|t| formats::nearest_pec(t.color)).collect()
}

fn pec_value(d: &mut Vec<u8>, v: i32, long: bool, flag: u8) {
    if !long && (-64..63).contains(&v) {
        d.push((v & 0x7f) as u8);
    } else {
        let v = (v & 0x0fff) as u16 | 0x8000 | ((flag as u16) << 8);
        d.extend_from_slice(&v.to_be_bytes());
    }
}

/// A 48 × 38 one-bit picture of the stitches with a frame, as PEC stores
/// for the machine's screen.
fn pec_icon(points: &[(f32, f32)], b: (i32, i32, i32, i32)) -> [u8; 228] {
    let mut g = [0u8; 228];
    let mut mark = |x: i32, y: i32| {
        if (0..48).contains(&x) && (0..38).contains(&y) {
            g[(y * 6 + x / 8) as usize] |= 1 << (x % 8);
        }
    };
    for x in 4..44 {
        mark(x, 2);
        mark(x, 35);
    }
    for y in 4..34 {
        mark(2, y);
        mark(45, y);
    }
    for (x, y) in [(3, 3), (44, 3), (3, 34), (44, 34)] {
        mark(x, y);
    }
    let (w, h) = ((b.2 - b.0).max(1) as f32, (b.3 - b.1).max(1) as f32);
    let scale = (36.0 / w).min(28.0 / h);
    let (cx, cy) = ((b.0 + b.2) as f32 / 2.0, (b.1 + b.3) as f32 / 2.0);
    for &(x, y) in points {
        mark(((x - cx) * scale + 24.0).floor() as i32, ((y - cy) * scale + 19.0).floor() as i32);
    }
    g
}

fn pec_block(name: &str, points: &[(f32, f32)], cmds: &[Cmd], threads: &[Thread]) -> Vec<u8> {
    let mut d = Vec::new();
    let label: String = name.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(8).collect();
    d.extend_from_slice(format!("LA:{label:<16}\r").as_bytes());
    d.extend_from_slice(&[b' '; 12]);
    d.extend_from_slice(&[0xff, 0x00, 6, 38]);
    d.extend_from_slice(&[b' '; 12]);
    let blocks = cmds.iter().filter(|c| **c == Cmd::ColorChange).count() + 1;
    let mut numbers = pec_numbers(threads);
    numbers.resize(blocks, numbers.last().copied().unwrap_or(20));
    d.push((blocks - 1) as u8);
    d.extend_from_slice(&numbers);
    d.resize(512, b' ');

    let (minx, miny, maxx, maxy) = stitch_bounds(points, cmds);
    let start = d.len();
    d.extend_from_slice(&[0, 0, 0, 0, 0, 0x31, 0xff, 0xf0]);
    d.extend_from_slice(&((maxx - minx) as u16).to_le_bytes());
    d.extend_from_slice(&((maxy - miny) as u16).to_le_bytes());
    d.extend_from_slice(&0x1e0u16.to_le_bytes());
    d.extend_from_slice(&0x1b0u16.to_le_bytes());
    // Brother's software starts with a long jump to the first point, and
    // readers skip these four bytes; a zero jump keeps both kinds right.
    pec_value(&mut d, 0, true, 0x10);
    pec_value(&mut d, 0, true, 0x10);
    let mut jumping = true;
    let mut init = true;
    let mut two = true;
    for (dx, dy, cmd, first) in moves(points, cmds, 2047) {
        match cmd {
            Cmd::Stitch => {
                jumping = false;
                pec_value(&mut d, dx, false, 0);
                pec_value(&mut d, dy, false, 0);
            }
            Cmd::Jump | Cmd::Trim | Cmd::ColorChange => {
                if cmd == Cmd::ColorChange && first {
                    d.extend_from_slice(&[0xfe, 0xb0, if two { 2 } else { 1 }]);
                    two = !two;
                }
                if dx != 0 || dy != 0 {
                    // The first move only positions the needle; later ones also cut.
                    let flag = if init { 0x10 } else { 0x20 };
                    pec_value(&mut d, dx, true, flag);
                    pec_value(&mut d, dy, true, flag);
                    jumping = true;
                }
            }
        }
        init = false;
    }
    let _ = jumping;
    d.push(0xff);
    let len = (d.len() - start) as u32;
    d[start + 2..start + 5].copy_from_slice(&len.to_le_bytes()[..3]);

    let b = (minx, miny, maxx, maxy);
    let stitched: Vec<(f32, f32)> = points.iter().zip(cmds).filter(|(_, c)| **c == Cmd::Stitch).map(|(p, _)| *p).collect();
    d.extend_from_slice(&pec_icon(&stitched, b));
    let mut block = Vec::new();
    for (i, (&p, &c)) in points.iter().zip(cmds).enumerate() {
        if c == Cmd::ColorChange && i > 0 {
            d.extend_from_slice(&pec_icon(&block, b));
            block.clear();
        }
        if c == Cmd::Stitch {
            block.push(p);
        }
    }
    d.extend_from_slice(&pec_icon(&block, b));
    d
}

fn vp3_string(d: &mut Vec<u8>, s: &str) {
    d.extend_from_slice(&(s.len() as u16).to_be_bytes());
    d.extend_from_slice(s.as_bytes());
}

fn vp3_string16(d: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    d.extend_from_slice(&((units.len() * 2) as u16).to_be_bytes());
    for u in units {
        d.extend_from_slice(&u.to_be_bytes());
    }
}

/// Starts a VP3 block: its tag and a length filled in by `vp3_close`.
fn vp3_open(d: &mut Vec<u8>, tag: u8) -> usize {
    d.extend_from_slice(&[0, tag, 0]);
    d.extend_from_slice(&[0; 4]);
    d.len() - 4
}

fn vp3_close(d: &mut [u8], at: usize) {
    let len = (d.len() - at - 4) as u32;
    d[at..at + 4].copy_from_slice(&len.to_be_bytes());
}

fn be32(d: &mut Vec<u8>, v: i32) {
    d.extend_from_slice(&v.to_be_bytes());
}

fn write_vp3(points: &[(f32, f32)], cmds: &[Cmd], threads: &[Thread]) -> Vec<u8> {
    const MAKER: &str = "Produced by     Software Ltd";
    let mut d = b"%vsm%\0".to_vec();
    vp3_string16(&mut d, MAKER);
    let file = vp3_open(&mut d, 2);
    vp3_string16(&mut d, "");
    let (minx, miny, maxx, maxy) = stitch_bounds(points, cmds);
    // Split into color blocks: (start, end) indices into points.
    let mut blocks = Vec::new();
    let mut s = 0;
    for i in 1..=cmds.len() {
        if i == cmds.len() || cmds[i] == Cmd::ColorChange {
            blocks.push((s, i));
            s = i;
        }
    }
    for v in [maxx * 100, -miny * 100, minx * 100, -maxy * 100] {
        be32(&mut d, v);
    }
    be32(&mut d, points.len() as i32);
    d.extend_from_slice(&[0, blocks.len() as u8, 12, 0, 1]);

    let design = vp3_open(&mut d, 3);
    let (w, h) = (maxx - minx, maxy - miny);
    let (cx, cy) = ((minx + maxx) / 2, (miny + maxy) / 2);
    be32(&mut d, cx * 100);
    be32(&mut d, -cy * 100);
    d.extend_from_slice(&[0, 0, 0]);
    for v in [-(w / 2) * 100, (w / 2) * 100, -(h / 2) * 100, (h / 2) * 100, w * 100, h * 100] {
        be32(&mut d, v);
    }
    vp3_string16(&mut d, "");
    d.extend_from_slice(&[0x64, 0x64]);
    for v in [4096, 0, 0, 4096] {
        be32(&mut d, v);
    }
    d.extend_from_slice(b"xxPP\x01\x00");
    vp3_string16(&mut d, MAKER);
    d.extend_from_slice(&(blocks.len() as u16).to_be_bytes());
    for (k, &(a, b)) in blocks.iter().enumerate() {
        let color = vp3_open(&mut d, 5);
        let first = if k == 0 { (0, 0) } else { (points[a].0.round() as i32, points[a].1.round() as i32) };
        let last = points.get(b.saturating_sub(1)).map_or((0, 0), |p| (p.0.round() as i32, p.1.round() as i32));
        be32(&mut d, (first.0 - cx) * 100);
        be32(&mut d, -(first.1 - cy) * 100);
        let t = threads.get(k).cloned().unwrap_or_else(|| formats::stock_thread(k));
        d.extend_from_slice(&[1, 0, t.color[0], t.color[1], t.color[2], 0, 0, 0, 5, 0x28]);
        vp3_string(&mut d, &t.code);
        vp3_string(&mut d, &t.name);
        vp3_string(&mut d, "");
        be32(&mut d, (last.0 - first.0) * 100);
        be32(&mut d, -(last.1 - first.1) * 100);
        let st = vp3_open(&mut d, 1);
        d.extend_from_slice(&[0x0a, 0xf6, 0x00]);
        let (mut x, mut y) = first;
        let mut jumping = false;
        for i in a..b {
            let (px, py) = (points[i].0.round() as i32, points[i].1.round() as i32);
            if cmds[i] != Cmd::Stitch {
                // Moves without a stitch are carried into the next stitch;
                // a cut first, so the long stitch that follows is a jump.
                if cmds[i] == Cmd::Jump && !jumping && i > a {
                    d.extend_from_slice(&[0x80, 0x03]);
                }
                jumping = cmds[i] == Cmd::Jump || jumping;
                continue;
            }
            let (dx, dy) = (px - x, py - y);
            if (-127..=127).contains(&dx) && (-127..=127).contains(&dy) {
                d.extend_from_slice(&[dx as i8 as u8, dy as i8 as u8]);
            } else {
                d.extend_from_slice(&[0x80, 0x01]);
                d.extend_from_slice(&(dx as i16).to_be_bytes());
                d.extend_from_slice(&(dy as i16).to_be_bytes());
                d.extend_from_slice(&[0x80, 0x02]);
            }
            (x, y) = (px, py);
            jumping = false;
        }
        vp3_close(&mut d, st);
        d.push(0);
        vp3_close(&mut d, color);
    }
    vp3_close(&mut d, design);
    vp3_close(&mut d, file);
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{Format, read};

    const HEART: &[u8] = include_bytes!("../tests/fixtures/heart_5x5_fill.jef");

    fn heart() -> (Vec<(f32, f32)>, Vec<Cmd>, Vec<Thread>) {
        let p = read("heart.jef", HEART).unwrap();
        let b = p.bounds();
        let (cx, cy) = (((b.0 + b.2) / 2.0).round(), ((b.1 + b.3) / 2.0).round());
        let pts = p.points.iter().map(|&(x, y)| (x - cx, y - cy)).collect();
        (pts, p.cmds.clone(), p.threads.clone())
    }

    /// Two colors with a long jump between them.
    fn two_colors() -> (Vec<(f32, f32)>, Vec<Cmd>, Vec<Thread>) {
        let pts = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (400.0, 20.0), (400.0, 20.0), (410.0, 30.0), (420.0, -80.0), (420.0, -300.0)];
        let cmds = vec![Cmd::Jump, Cmd::Stitch, Cmd::Stitch, Cmd::Jump, Cmd::ColorChange, Cmd::Stitch, Cmd::Stitch, Cmd::Jump];
        let threads = vec![formats::jef_thread(1, 0), formats::jef_thread(10, 1)];
        (pts, cmds, threads)
    }

    fn stitched(pts: &[(f32, f32)], cmds: &[Cmd]) -> Vec<(f32, f32)> {
        pts.iter().zip(cmds).filter(|(_, c)| **c == Cmd::Stitch).map(|(p, _)| *p).collect()
    }

    #[test]
    fn every_format_reads_back_the_same_stitches() {
        for (pts, cmds, threads) in [heart(), two_colors()] {
            let want = stitched(&pts, &cmds);
            for out in [Out::Jef, Out::Dst, Out::Exp, Out::Pes, Out::Pec] {
                let bytes = write(out, "Heart", &pts, &cmds, &threads);
                let q = read(&format!("x.{}", out.ext()), &bytes).unwrap_or_else(|e| panic!("{out:?}: {e}"));
                let got = stitched(&q.points, &q.cmds);
                assert_eq!(got.len(), want.len(), "{out:?}");
                // Readers start at the origin; compare shapes, not offsets.
                let (ox, oy) = (got[0].0 - want[0].0, got[0].1 - want[0].1);
                for (g, w) in got.iter().zip(&want) {
                    assert!((g.0 - ox - w.0).abs() < 0.6 && (g.1 - oy - w.1).abs() < 0.6, "{out:?} {g:?} {w:?}");
                }
                assert_eq!(q.count(Cmd::ColorChange), cmds.iter().filter(|c| **c == Cmd::ColorChange).count(), "{out:?}");
            }
        }
    }

    #[test]
    fn pes_keeps_brother_colors() {
        let (pts, cmds, threads) = two_colors();
        let q = read("x.pes", &write(Out::Pes, "T", &pts, &cmds, &threads)).unwrap();
        assert_eq!(q.format, Format::Pes);
        assert_eq!(q.threads[0].name, "Black");
        assert_eq!(q.threads[1].name, "Red");
    }

    #[test]
    fn dst_header_has_size_and_counts() {
        let (pts, cmds, threads) = two_colors();
        let d = write(Out::Dst, "Two", &pts, &cmds, &threads);
        let h = String::from_utf8_lossy(&d[..120]).into_owned();
        assert!(h.starts_with("LA:Two             \rST:"), "{h}");
        assert!(h.contains("CO:  1\r"), "{h}");
        assert!(h.contains("+X:  420\r") && h.contains("+Y:   80\r") && h.contains("-Y:   30\r"), "{h}");
        assert_eq!(d.len() % 3, 512 % 3);
    }

    #[test]
    fn vp3_has_blocks_for_each_color() {
        let (pts, cmds, threads) = heart();
        let d = write(Out::Vp3, "Heart", &pts, &cmds, &threads);
        assert!(d.starts_with(b"%vsm%\0"));
        // The file block's length reaches the end of the file.
        let at = 6 + 2 + 2 * 28 + 3;
        let len = u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as usize;
        assert_eq!(at + 4 + len, d.len());
    }

    /// Writes the heart in every format into $HOOP_OUT for checking with
    /// other readers.
    #[test]
    #[ignore]
    fn export_samples() {
        let out = std::env::var("HOOP_OUT").unwrap_or_else(|_| "/tmp".into());
        for (name, (pts, cmds, threads)) in [("heart", heart()), ("two", two_colors())] {
            for o in Out::ALL {
                std::fs::write(std::path::Path::new(&out).join(format!("{name}.{}", o.ext())), write(o, name, &pts, &cmds, &threads)).unwrap();
            }
        }
    }
}
