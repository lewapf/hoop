//! Threads for the drawing's colors: grouping by thread and merging down
//! to fewer threads.

use super::*;

/// SVG colors grouped by the thread they use, in order of first use.
pub fn thread_groups(art: &Artwork, s: &ArtSettings) -> Vec<(i32, Vec<usize>)> {
    let mut groups: Vec<(i32, Vec<usize>)> = Vec::new();
    for (i, l) in s.layers.iter().enumerate() {
        // Backgrounds stay on their own row so they can be switched off alone.
        let key = if art.layers[i].background { -l.thread } else { l.thread };
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    for g in &mut groups {
        g.0 = g.0.abs();
    }
    groups
}

/// Merges the closest threads until at most `max` remain, keeping the thread
/// used by more shapes in each merge.
pub fn reduce_threads(art: &Artwork, s: &mut ArtSettings, max: usize) {
    loop {
        let mut used: Vec<(i32, usize)> = Vec::new();
        for (i, l) in s.layers.iter().enumerate() {
            if !l.enabled {
                continue;
            }
            match used.iter_mut().find(|u| u.0 == l.thread) {
                Some(u) => u.1 += art.layers[i].shapes,
                None => used.push((l.thread, art.layers[i].shapes)),
            }
        }
        if used.len() <= max.max(1) {
            return;
        }
        let color = |t: i32| formats::jef_thread(t, 0).color;
        let d = |a: [u8; 3], b: [u8; 3]| {
            let (r, g, bb) = (a[0] as i32 - b[0] as i32, a[1] as i32 - b[1] as i32, a[2] as i32 - b[2] as i32);
            2 * r * r + 4 * g * g + 3 * bb * bb
        };
        let mut best = (i32::MAX, 0, 1);
        for i in 0..used.len() {
            for j in i + 1..used.len() {
                let dist = d(color(used[i].0), color(used[j].0));
                if dist < best.0 {
                    best = (dist, i, j);
                }
            }
        }
        let (keep, drop) = if used[best.1].1 >= used[best.2].1 { (used[best.1].0, used[best.2].0) } else { (used[best.2].0, used[best.1].0) };
        for l in &mut s.layers {
            if l.thread == drop {
                l.thread = keep;
            }
        }
    }
}
