//! Machines, their hoops and the fit check. Sizes are the sewing area in
//! mm, width across the machine and height front to back.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::export::Out;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hoop {
    pub name: &'static str,
    pub w: f32,
    pub h: f32,
    pub note: &'static str,
    /// Picked on its own for a design that fits; free-arm and the big
    /// hoops only some machines take are picked by hand.
    pub auto: bool,
}

/// A family of machines that read one file format and share hoops.
pub struct Machine {
    pub key: &'static str,
    pub name: &'static str,
    pub out: Out,
    /// Where the machine looks for designs on a USB stick, folder by folder;
    /// empty for the top of the stick.
    pub usb_folder: &'static [&'static str],
    pub hoops: &'static [Hoop],
}

const fn hoop(name: &'static str, w: f32, h: f32, note: &'static str, auto: bool) -> Hoop {
    Hoop { name, w, h, note, auto }
}

pub const MACHINES: [Machine; 5] = [
    Machine {
        key: "janome",
        name: "Janome",
        out: Out::Jef,
        usb_folder: &["EMB", "Embf"],
        hoops: &[
            hoop("SQ14b", 140.0, 140.0, n!("Standard"), true),
            hoop("RE20b", 170.0, 200.0, n!("Standard"), true),
            hoop("SQ20b", 200.0, 200.0, n!("Optional"), true),
            hoop("FA10a", 100.0, 40.0, n!("Free arm"), false),
            hoop("RE28b", 200.0, 280.0, n!("Large machines"), false),
            hoop("RE36b", 200.0, 360.0, n!("Large machines"), false),
        ],
    },
    Machine {
        key: "brother",
        name: "Brother · Baby Lock",
        out: Out::Pes,
        usb_folder: &[],
        hoops: &[
            hoop("4×4\"", 100.0, 100.0, n!("Standard"), true),
            hoop("5×7\"", 130.0, 180.0, n!("Standard"), true),
            hoop("6×10\"", 160.0, 260.0, n!("Optional"), true),
            hoop("7×12\"", 180.0, 300.0, n!("Large machines"), false),
            hoop("8×12\"", 200.0, 300.0, n!("Large machines"), false),
        ],
    },
    Machine {
        key: "pfaff",
        name: "Pfaff · Husqvarna Viking",
        out: Out::Vp3,
        usb_folder: &[],
        hoops: &[
            hoop("Square", 120.0, 120.0, n!("Standard"), true),
            hoop("Master", 240.0, 150.0, n!("Standard"), true),
            hoop("Elite", 260.0, 200.0, n!("Optional"), true),
            hoop("Grand Dream", 360.0, 200.0, n!("Large machines"), false),
            hoop("Deluxe", 360.0, 350.0, n!("Large machines"), false),
        ],
    },
    Machine {
        key: "bernina",
        name: "Bernina",
        out: Out::Exp,
        usb_folder: &[],
        hoops: &[
            hoop("Small", 72.0, 50.0, n!("Optional"), false),
            hoop("Medium", 100.0, 130.0, n!("Standard"), true),
            hoop("Large Oval", 145.0, 255.0, n!("Standard"), true),
            hoop("Maxi", 210.0, 400.0, n!("Large machines"), false),
        ],
    },
    Machine {
        key: "other",
        name: n!("Other machine (DST)"),
        out: Out::Dst,
        usb_folder: &[],
        hoops: &[
            hoop("4×4\"", 100.0, 100.0, n!("Common"), true),
            hoop("5×7\"", 130.0, 180.0, n!("Common"), true),
            hoop("6×10\"", 160.0, 260.0, n!("Common"), true),
            hoop("8×8\"", 200.0, 200.0, n!("Common"), true),
            hoop("8×12\"", 200.0, 300.0, n!("Common"), true),
        ],
    },
];

static MACHINE: AtomicUsize = AtomicUsize::new(0);

/// The machine picked in settings; its hoops are the ones shown.
pub fn machine() -> &'static Machine {
    &MACHINES[MACHINE.load(Ordering::Relaxed).min(MACHINES.len() - 1)]
}

pub fn machine_index() -> usize {
    MACHINE.load(Ordering::Relaxed).min(MACHINES.len() - 1)
}

pub fn set_machine(i: usize) {
    MACHINE.store(i.min(MACHINES.len() - 1), Ordering::Relaxed);
}

/// The current machine's hoops.
pub fn list() -> &'static [Hoop] {
    machine().hoops
}

/// A hoop of the current machine, the first one for an index out of range.
pub fn get(i: usize) -> Hoop {
    list().get(i).copied().unwrap_or(list()[0])
}

/// Where the design sits: rotation in quarter turns, mirroring, and the offset
/// of its center from the hoop center in 0.1 mm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    pub quarter_turns: u8,
    pub mirror: bool,
    pub dx: f32,
    pub dy: f32,
}

impl Placement {
    /// Design size after rotation, in 0.1 mm.
    pub fn size(&self, w: f32, h: f32) -> (f32, f32) {
        if self.quarter_turns.is_multiple_of(2) { (w, h) } else { (h, w) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    /// Fits; the value is the distance to the closest edge in mm.
    Inside(f32),
    /// Too big for the hoop: overflow across and front to back in mm, and
    /// whether a quarter turn would make it fit.
    TooBig { over_w: f32, over_h: f32, fits_turned: bool },
    /// Small enough, but moved past an edge by this many mm.
    OffEdge { side: &'static str, by: f32 },
}

/// `design` is the unrotated design size in 0.1 mm.
pub fn fit(hoop: (f32, f32), design: (f32, f32), at: &Placement) -> Fit {
    let (hw, hh) = (hoop.0 * 10.0, hoop.1 * 10.0);
    let (dw, dh) = at.size(design.0, design.1);
    if dw > hw || dh > hh {
        return Fit::TooBig { over_w: ((dw - hw) / 10.0).max(0.0), over_h: ((dh - hh) / 10.0).max(0.0), fits_turned: dh <= hw && dw <= hh };
    }
    let margins = [
        ("left", hw / 2.0 + at.dx - dw / 2.0),
        ("right", hw / 2.0 - at.dx - dw / 2.0),
        ("top", hh / 2.0 + at.dy - dh / 2.0),
        ("bottom", hh / 2.0 - at.dy - dh / 2.0),
    ];
    let (side, m) = margins.into_iter().fold(("", f32::MAX), |a, b| if b.1 < a.1 { b } else { a });
    if m < 0.0 { Fit::OffEdge { side, by: -m / 10.0 } } else { Fit::Inside(m / 10.0) }
}

/// The smallest hoop of the current machine the design fits without
/// turning it, or the biggest one when nothing fits. Only hoops marked
/// `auto` are picked.
pub fn smallest_fitting(design: (f32, f32)) -> usize {
    smallest_in(list(), design)
}

fn smallest_in(hoops: &[Hoop], design: (f32, f32)) -> usize {
    let mut auto: Vec<usize> = (0..hoops.len()).filter(|&i| hoops[i].auto).collect();
    auto.sort_by(|&a, &b| (hoops[a].w * hoops[a].h).total_cmp(&(hoops[b].w * hoops[b].h)));
    auto.iter().copied().find(|&i| matches!(fit((hoops[i].w, hoops[i].h), design, &Placement::default()), Fit::Inside(_))).or(auto.last().copied()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heart_fits_square_hoop_with_margin() {
        let f = fit((140.0, 140.0), (1272.0, 1274.0), &Placement::default());
        assert!(matches!(f, Fit::Inside(m) if (m - 6.3).abs() < 0.01), "{f:?}");
        assert_eq!(smallest_fitting((1272.0, 1274.0)), 0);
    }

    #[test]
    fn wide_design_fits_only_when_turned() {
        let f = fit((140.0, 200.0), (1800.0, 1000.0), &Placement::default());
        assert_eq!(f, Fit::TooBig { over_w: 40.0, over_h: 0.0, fits_turned: true });
        let turned = Placement { quarter_turns: 1, ..Default::default() };
        assert!(matches!(fit((140.0, 200.0), (1800.0, 1000.0), &turned), Fit::Inside(_)));
    }

    #[test]
    fn moved_design_reports_the_edge() {
        let at = Placement { dx: 100.0, ..Default::default() };
        assert_eq!(fit((140.0, 140.0), (1300.0, 500.0), &at), Fit::OffEdge { side: "right", by: 5.0 });
    }

    #[test]
    fn re20b_takes_designs_wider_than_the_square_hoop() {
        // 160 mm wide is too wide for SQ14b but fits the 170 × 200 mm RE20b.
        assert_eq!(smallest_fitting((1600.0, 1000.0)), 1);
    }

    #[test]
    fn every_machine_has_an_automatic_hoop() {
        for m in &MACHINES {
            assert!(m.hoops.iter().any(|h| h.auto), "{}", m.key);
            // A 5 × 5 cm heart fits the smallest automatic hoop.
            let i = smallest_in(m.hoops, (500.0, 500.0));
            assert!(m.hoops[i].auto && m.hoops[i].w >= 50.0);
        }
        // Brother: 15 × 20 cm needs the 6×10" hoop.
        assert_eq!(smallest_in(MACHINES[1].hoops, (1500.0, 2000.0)), 2);
    }

    #[test]
    fn oversized_design_picks_biggest_hoop() {
        assert_eq!(smallest_fitting((2500.0, 900.0)), 2);
        assert_eq!(smallest_fitting((1300.0, 1800.0)), 1);
    }
}
