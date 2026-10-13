//! Hoop projects (.hoop): everything needed to carry on later where the
//! work stopped. An artwork project keeps the drawing as Hoop read it plus
//! every edit (shapes drawn, moved or restyled, groups, attached lines);
//! a lettering project keeps the text and its settings. Parts pinned to
//! the hoop come along as stitches. Stored as JSON so it stays readable.

use serde::{Deserialize, Serialize};

use crate::artwork::{ArtSettings, Artwork};
use crate::formats::Pattern;
use crate::hoops::Placement;
use crate::lettering::Settings;

/// The file format's version; newer files may add fields, which older
/// fields default around.
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub version: u32,
    /// "artwork" or "lettering".
    pub mode: String,
    /// The machine family's key, like "janome".
    pub machine: String,
    pub hoop: usize,
    pub at: Placement,
    #[serde(default)]
    pub artwork: Option<Artwork>,
    #[serde(default)]
    pub art: Option<ArtSettings>,
    #[serde(default)]
    pub letter: Option<Settings>,
    /// The lettering font by name, since font numbers change as fonts are added.
    #[serde(default)]
    pub font: String,
    #[serde(default)]
    pub pinned: Vec<PinnedPart>,
    /// In an autosave: the project file the work came from, if any.
    #[serde(default)]
    pub origin: Option<String>,
}

/// A finished part pinned to the hoop, in hoop coordinates.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PinnedPart {
    pub name: String,
    pub pattern: Pattern,
}

impl Project {
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).unwrap_or_default()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Project, String> {
        let p: Project = serde_json::from_slice(data).map_err(|e| t!("This isn't a Hoop project Hoop can read: {e}.", e = e))?;
        if p.version > VERSION {
            return Err(t!("This project comes from a newer Hoop. Update Hoop to open it.").to_string());
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artwork;

    #[test]
    fn artwork_projects_keep_every_edit() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
            <rect x="0" y="0" width="20" height="20" fill="#ff0000"/>
            <circle cx="80" cy="10" r="10" fill="#0000ff"/>
        </svg>"##;
        let art = artwork::parse("two.svg", svg.as_bytes()).unwrap();
        let mut s = ArtSettings::for_artwork(&art);
        s.shapes[0].dx = 12.5;
        s.shapes[1].name = "Ball".into();
        s.shapes[0].group = Some(3);
        s.groups.push(artwork::Group { id: 3, name: "Left".into() });
        s.shapes[1].made = Some(artwork::Made::Star { w: 10.0, h: 10.0, points: 5, inner: 0.4 });
        let p = Project {
            version: VERSION,
            mode: "artwork".into(),
            machine: "janome".into(),
            hoop: 2,
            at: Placement { quarter_turns: 1, mirror: true, dx: 5.0, dy: -3.0 },
            artwork: Some(art.clone()),
            art: Some(s.clone()),
            letter: None,
            font: String::new(),
            pinned: Vec::new(),
            origin: None,
        };
        let back = Project::from_bytes(&p.to_bytes()).unwrap();
        assert_eq!(back.art.as_ref(), Some(&s));
        assert_eq!(back.artwork.as_ref().map(|a| a.shapes.clone()), Some(art.shapes.clone()));
        assert_eq!((back.hoop, back.at.quarter_turns, back.at.mirror), (2, 1, true));
        // The drawing stitches the same from the project as from the edits.
        let (a, b) = (artwork::generate(&art, &s).unwrap(), artwork::generate(back.artwork.as_ref().unwrap(), back.art.as_ref().unwrap()).unwrap());
        assert_eq!(a.points, b.points);
    }

    #[test]
    fn rejects_other_files_and_newer_projects() {
        assert!(Project::from_bytes(b"<svg/>").is_err());
        let newer = br#"{"version": 99, "mode": "artwork", "machine": "janome", "hoop": 0, "at": {"quarter_turns": 0, "mirror": false, "dx": 0, "dy": 0}}"#;
        assert!(Project::from_bytes(newer).unwrap_err().contains("newer"));
        // Older files without the optional parts still open.
        let minimal = br#"{"version": 1, "mode": "lettering", "machine": "janome", "hoop": 0, "at": {"quarter_turns": 0, "mirror": false, "dx": 0, "dy": 0}}"#;
        assert!(Project::from_bytes(minimal).unwrap().letter.is_none());
    }
}
