//! Janome MC9900 hoops and the fit check. Sizes are the sewing area in mm,
//! width across the machine and height front to back.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hoop {
    pub name: &'static str,
    pub w: f32,
    pub h: f32,
    pub note: &'static str,
}

pub const HOOPS: [Hoop; 4] = [
    Hoop { name: "SQ14b", w: 140.0, h: 140.0, note: "Standard" },
    Hoop { name: "RE20b", w: 170.0, h: 200.0, note: "Standard" },
    Hoop { name: "SQ20b", w: 200.0, h: 200.0, note: "Optional" },
    Hoop { name: "FA10a", w: 100.0, h: 40.0, note: "Free arm" },
];

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
        if self.quarter_turns % 2 == 0 { (w, h) } else { (h, w) }
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
        return Fit::TooBig {
            over_w: ((dw - hw) / 10.0).max(0.0),
            over_h: ((dh - hh) / 10.0).max(0.0),
            fits_turned: dh <= hw && dw <= hh,
        };
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

/// The smallest standard hoop the design fits without turning it, or the
/// biggest one when nothing fits. The free-arm hoop is never picked on its own.
pub fn smallest_fitting(design: (f32, f32)) -> usize {
    [0, 1, 2]
        .into_iter()
        .find(|&i| matches!(fit((HOOPS[i].w, HOOPS[i].h), design, &Placement::default()), Fit::Inside(_)))
        .unwrap_or(2)
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
    fn oversized_design_picks_biggest_hoop() {
        assert_eq!(smallest_fitting((2500.0, 900.0)), 2);
        assert_eq!(smallest_fitting((1300.0, 1800.0)), 1);
    }
}
