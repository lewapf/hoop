//! Production worksheet: a two-page PDF with the design's facts, its thread
//! sequence and a picture, then the design at actual size with center marks
//! for placing it on the fabric. Written by hand: text in the PDF base
//! fonts and stitches as vector lines, so no PDF library is needed.

use crate::formats::{Cmd, Thread};

pub struct Sheet<'a> {
    pub name: &'a str,
    /// Thread chart the threads come from, for the table heading.
    pub chart: &'a str,
    pub hoop: &'a str,
    /// Points in 0.1 mm, y down, centered on the hoop.
    pub points: &'a [(f32, f32)],
    pub cmds: &'a [Cmd],
    /// One thread per color block.
    pub threads: &'a [Thread],
    /// Machine speed in stitches per minute.
    pub spm: u32,
    /// Date shown in the header, like "2026-10-12".
    pub date: &'a str,
}

const W: f32 = 595.28;
const H: f32 = 841.89;
const MM: f32 = 72.0 / 25.4;

/// Text for a PDF string in WinAnsi encoding: escapes and the few
/// non-ASCII characters the sheet uses.
fn pdf_text(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '×' => out.push_str("\\327"),
            '·' => out.push_str("\\267"),
            '–' => out.push_str("\\226"),
            '≈' => out.push('~'),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c if (c as u32) < 256 => out.push_str(&format!("\\{:03o}", c as u32)),
            // The rest of WinAnsi, then plain letters for accents it lacks
            // (Polish, Czech, Turkish, Romanian…), since the base fonts
            // can't show them.
            'Š' => out.push_str("\\212"),
            'š' => out.push_str("\\232"),
            'Ž' => out.push_str("\\216"),
            'ž' => out.push_str("\\236"),
            'Œ' => out.push_str("\\214"),
            'œ' => out.push_str("\\234"),
            '—' => out.push_str("\\227"),
            '…' => out.push_str("\\205"),
            '„' => out.push_str("\\204"),
            '“' => out.push_str("\\223"),
            '”' => out.push_str("\\224"),
            '€' => out.push_str("\\200"),
            c => out.push(plain(c)),
        }
    }
    out
}

/// A letter without its accent, for letters WinAnsi doesn't have.
fn plain(c: char) -> char {
    const MAP: &[(&str, char)] = &[
        ("ąăāа", 'a'),
        ("ĄĂĀ", 'A'),
        ("ćčĉċ", 'c'),
        ("ĆČ", 'C'),
        ("ďđ", 'd'),
        ("ĎĐ", 'D'),
        ("ęěėē", 'e'),
        ("ĘĚĖĒ", 'E'),
        ("ğ", 'g'),
        ("Ğ", 'G'),
        ("ıīį", 'i'),
        ("İĪĮ", 'I'),
        ("łľĺ", 'l'),
        ("ŁĽĹ", 'L'),
        ("ńňņ", 'n'),
        ("ŃŇŅ", 'N'),
        ("őō", 'o'),
        ("ŐŌ", 'O'),
        ("řŕ", 'r'),
        ("ŘŔ", 'R'),
        ("śşș", 's'),
        ("ŚŞȘ", 'S'),
        ("ťţț", 't'),
        ("ŤŢȚ", 'T'),
        ("ůűūų", 'u'),
        ("ŮŰŪŲ", 'U'),
        ("źż", 'z'),
        ("ŹŻ", 'Z'),
    ];
    MAP.iter().find(|(set, _)| set.contains(c)).map_or('?', |(_, p)| *p)
}

struct Page {
    s: String,
}

impl Page {
    fn new() -> Page {
        Page { s: String::new() }
    }

    /// Text at (x, y) in points from the top-left corner.
    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, s: &str) {
        let font = if bold { "F2" } else { "F1" };
        self.s.push_str(&format!("BT /{font} {size} Tf {x:.2} {:.2} Td ({}) Tj ET\n", H - y, pdf_text(s)));
    }

    fn gray(&mut self, g: f32) {
        self.s.push_str(&format!("{g:.3} g {g:.3} G\n"));
    }

    fn rgb(&mut self, c: [u8; 3]) {
        let [r, g, b] = c.map(|v| v as f32 / 255.0);
        self.s.push_str(&format!("{r:.3} {g:.3} {b:.3} rg {r:.3} {g:.3} {b:.3} RG\n"));
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: bool) {
        self.s.push_str(&format!("{x:.2} {:.2} {w:.2} {h:.2} re {}\n", H - y - h, if fill { "f" } else { "S" }));
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        self.s.push_str(&format!("{:.2} {:.2} m {:.2} {:.2} l S\n", a.0, H - a.1, b.0, H - b.1));
    }

    fn width(&mut self, w: f32) {
        self.s.push_str(&format!("{w:.2} w\n"));
    }
}

/// Thread color as it shows on paper: very light threads are drawn gray so
/// they don't vanish on white.
fn ink(c: [u8; 3]) -> [u8; 3] {
    let l = 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
    if l > 225.0 { [190, 190, 190] } else { c }
}

/// Color blocks as (start, end) indices into the points.
fn blocks(cmds: &[Cmd]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut s = 0;
    for i in 1..=cmds.len() {
        if i == cmds.len() || cmds[i] == Cmd::ColorChange {
            if cmds[s..i].contains(&Cmd::Stitch) {
                out.push((s, i));
            }
            s = i;
        }
    }
    out
}

fn bounds(points: &[(f32, f32)], cmds: &[Cmd]) -> (f32, f32, f32, f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (&(x, y), _) in points.iter().zip(cmds).filter(|(_, c)| **c == Cmd::Stitch) {
        b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    }
    b
}

/// Draws the stitches with `map` taking design units to page points.
fn stitches(page: &mut Page, sheet: &Sheet, line_w: f32, map: &dyn Fn((f32, f32)) -> (f32, f32)) {
    page.width(line_w);
    page.s.push_str("1 J 1 j\n");
    for (k, (a, b)) in blocks(sheet.cmds).into_iter().enumerate() {
        let thread = sheet.threads.get(k).map_or([0, 0, 0], |t| t.color);
        page.rgb(ink(thread));
        let mut open = false;
        for i in a.max(1)..b {
            if sheet.cmds[i] != Cmd::Stitch {
                open = false;
                continue;
            }
            let p = map(sheet.points[i]);
            if !open {
                let q = map(sheet.points[i - 1]);
                page.s.push_str(&format!("{:.2} {:.2} m ", q.0, H - q.1));
                open = true;
            }
            page.s.push_str(&format!("{:.2} {:.2} l ", p.0, H - p.1));
            if i % 200 == 0 {
                // Keep paths short for slow PDF viewers.
                page.s.push_str("S\n");
                page.s.push_str(&format!("{:.2} {:.2} m ", p.0, H - p.1));
            }
        }
        page.s.push_str("S\n");
    }
}

fn meters(points: &[(f32, f32)], cmds: &[Cmd], a: usize, b: usize) -> f32 {
    let mut sum = 0.0;
    for i in a.max(1)..b {
        if cmds[i] == Cmd::Stitch {
            let (p, q) = (points[i - 1], points[i]);
            sum += ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
        }
    }
    sum / 10_000.0 * 1.2
}

pub fn pdf(sheet: &Sheet) -> Vec<u8> {
    let b = bounds(sheet.points, sheet.cmds);
    let (w, h) = ((b.2 - b.0) / 10.0, (b.3 - b.1) / 10.0);
    let stitch_count = sheet.cmds.iter().filter(|c| **c == Cmd::Stitch).count();
    let jumps = sheet.cmds.iter().filter(|c| **c == Cmd::Jump).count();
    let bl = blocks(sheet.cmds);
    let minutes = stitch_count as f32 / sheet.spm.max(1) as f32;
    let total_m: f32 = bl.iter().map(|&(a, e)| meters(sheet.points, sheet.cmds, a, e)).sum();

    // Page 1: facts, picture, thread sequence.
    let mut p = Page::new();
    let m = 18.0 * MM;
    p.gray(0.1);
    p.text(m, m + 6.0, 20.0, true, sheet.name);
    p.gray(0.45);
    p.text(m, m + 24.0, 10.0, false, &t!("Embroidery worksheet · {date} · made with Hoop", date = sheet.date));
    p.width(0.6);
    p.line((m, m + 34.0), (W - m, m + 34.0));

    // Picture box on the left.
    let bx = (m, m + 48.0, 92.0 * MM, 92.0 * MM);
    p.gray(0.95);
    p.rect(bx.0, bx.1, bx.2, bx.3, true);
    let scale = ((bx.2 - 12.0) / (b.2 - b.0).max(1.0)).min((bx.3 - 12.0) / (b.3 - b.1).max(1.0));
    let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    let center = (bx.0 + bx.2 / 2.0, bx.1 + bx.3 / 2.0);
    stitches(&mut p, sheet, 0.3, &|(x, y)| (center.0 + (x - cx) * scale, center.1 + (y - cy) * scale));

    // Facts on the right.
    let fx = bx.0 + bx.2 + 10.0 * MM;
    let changes = bl.len().saturating_sub(1);
    let facts = [
        (t!("Size"), t!("{w:.1} × {h:.1} mm", w = w, h = h)),
        (t!("Hoop"), sheet.hoop.to_string()),
        (t!("Stitches"), stitch_count.to_string()),
        (
            t!("Colors"),
            if changes == 0 { "1".into() } else { tn!(changes, "{colors} ({n} thread change)", "{colors} ({n} thread changes)", colors = bl.len()) },
        ),
        (t!("Jumps"), jumps.to_string()),
        (t!("Sewing time"), t!("≈ {v0:.0} min at {spm} spm", v0 = minutes.max(1.0), spm = sheet.spm)),
        (t!("Top thread"), t!("≈ {m:.1} m", m = total_m)),
        (t!("Center offset"), t!("{v0:+.1}, {v1:+.1} mm", v0 = cx / 10.0, v1 = cy / 10.0)),
    ];
    let mut y = bx.1 + 10.0;
    for (k, v) in facts {
        p.gray(0.45);
        p.text(fx, y, 9.0, false, k);
        p.gray(0.1);
        p.text(fx, y + 13.0, 12.0, true, &v);
        y += 33.0;
    }

    // Thread sequence.
    let mut y = bx.1 + bx.3 + 30.0;
    p.gray(0.1);
    p.text(m, y, 13.0, true, t!("Thread sequence"));
    y += 18.0;
    p.gray(0.45);
    let cols = [m, m + 22.0, m + 50.0, m + 260.0, m + 340.0, m + 410.0];
    for (x, t) in cols.iter().zip(["#", "", &t!("Thread ({chart})", chart = sheet.chart), t!("Code"), t!("Stitches"), t!("Length")]) {
        p.text(*x, y, 9.0, false, t);
    }
    y += 6.0;
    p.width(0.4);
    p.line((m, y), (W - m, y));
    y += 16.0;
    for (k, &(a, e)) in bl.iter().enumerate() {
        if y > H - m {
            p.gray(0.45);
            p.text(m, y, 9.0, false, &t!("… and {v0} more", v0 = bl.len() - k));
            break;
        }
        let t = sheet.threads.get(k).cloned().unwrap_or_else(|| crate::formats::stock_thread(k));
        let n = sheet.cmds[a..e].iter().filter(|c| **c == Cmd::Stitch).count();
        p.gray(0.1);
        p.text(cols[0], y, 10.0, false, &(k + 1).to_string());
        p.rgb(t.color);
        p.rect(cols[1], y - 9.0, 18.0, 11.0, true);
        p.gray(0.6);
        p.width(0.4);
        p.rect(cols[1], y - 9.0, 18.0, 11.0, false);
        p.gray(0.1);
        p.text(cols[2], y, 10.0, false, &t.name);
        p.text(cols[3], y, 10.0, false, &t.code);
        p.text(cols[4], y, 10.0, false, &n.to_string());
        p.text(cols[5], y, 10.0, false, &format!("{:.1} m", meters(sheet.points, sheet.cmds, a, e)));
        y += 18.0;
    }

    // Page 2: actual size template.
    let mut q = Page::new();
    q.gray(0.1);
    q.text(m, m + 6.0, 14.0, true, &t!("{name} · actual size", name = sheet.name));
    q.gray(0.45);
    q.text(
        m,
        m + 22.0,
        9.0,
        false,
        t!("Print at 100 % (no fit to page). The bar below must measure 50 mm. Lay the sheet on the fabric and mark the center cross."),
    );
    q.width(1.0);
    q.gray(0.1);
    let bar_y = m + 36.0;
    q.line((m, bar_y), (m + 50.0 * MM, bar_y));
    for k in 0..=5 {
        let x = m + k as f32 * 10.0 * MM;
        q.line((x, bar_y - 4.0), (x, bar_y + 4.0));
    }
    q.text(m + 50.0 * MM + 6.0, bar_y + 3.0, 9.0, false, t!("50 mm"));
    // The hoop center goes in the middle of the free area.
    let area = (m + 48.0, H - m);
    let o = (W / 2.0, (area.0 + area.1) / 2.0);
    let fits = (b.2 - b.0) / 10.0 * MM < W - 10.0 * MM && (b.3 - b.1) / 10.0 * MM < area.1 - area.0;
    if !fits {
        q.text(m, bar_y + 20.0, 9.0, false, t!("The design is larger than the page, so its edges are cut off here."));
    }
    let map = |(x, y): (f32, f32)| (o.0 + x / 10.0 * MM, o.1 + y / 10.0 * MM);
    stitches(&mut q, sheet, 0.35, &map);
    // Hoop center cross and the design's own center.
    q.gray(0.1);
    q.width(0.5);
    q.line((o.0 - 12.0 * MM, o.1), (o.0 + 12.0 * MM, o.1));
    q.line((o.0, o.1 - 12.0 * MM), (o.0, o.1 + 12.0 * MM));
    q.text(o.0 + 2.0, o.1 - 12.0 * MM - 2.0, 7.0, false, t!("hoop center"));
    q.gray(0.55);
    q.s.push_str("[3 2] 0 d\n");
    q.rect(map((b.0, b.1)).0, map((b.0, b.1)).1, (b.2 - b.0) / 10.0 * MM, (b.3 - b.1) / 10.0 * MM, false);
    q.s.push_str("[] 0 d\n");

    assemble(&[p.s, q.s])
}

fn assemble(pages: &[String]) -> Vec<u8> {
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        String::new(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".into(),
    ];
    let mut kids = Vec::new();
    for content in pages {
        let page = objects.len() + 1;
        kids.push(format!("{page} 0 R"));
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
            page + 1
        ));
        objects.push(format!("<< /Length {} >>\nstream\n{content}endstream", content.len()));
    }
    objects[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len());
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::read;

    const HEART: &[u8] = include_bytes!("../tests/fixtures/heart_5x5_fill.jef");

    fn heart_pdf() -> Vec<u8> {
        let p = read("heart.jef", HEART).unwrap();
        let threads: Vec<Thread> = p.blocks().into_iter().map(|b| b.thread).collect();
        pdf(&Sheet {
            name: "Heart (5×5)",
            chart: "Janome",
            hoop: "SQ14b 140 × 140 mm",
            points: &p.points,
            cmds: &p.cmds,
            threads: &threads,
            spm: 600,
            date: "2026-10-12",
        })
    }

    #[test]
    fn text_falls_back_to_plain_letters() {
        assert_eq!(pdf_text("Łódź (5×5) Šip…"), "L\\363dz \\(5\\3275\\) \\212ip\\205");
    }

    #[test]
    fn pdf_has_two_pages_and_a_valid_xref() {
        let d = heart_pdf();
        let s = String::from_utf8_lossy(&d);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("/Count 2"));
        assert!(s.contains("(Heart \\(5\\3275\\)) Tj"));
        // startxref points at the xref table.
        let at: usize = s.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        assert!(d[at..].starts_with(b"xref"));
        // Every object offset points at its "n 0 obj".
        let table = String::from_utf8_lossy(&d[at..]).into_owned();
        for (i, line) in table.lines().skip(3).take(6).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(d[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "object {}", i + 1);
        }
    }

    #[test]
    #[ignore]
    fn worksheet_sample() {
        let out = std::env::var("HOOP_OUT").unwrap_or_else(|_| "/tmp".into());
        std::fs::write(std::path::Path::new(&out).join("worksheet.pdf"), heart_pdf()).unwrap();
    }
}
