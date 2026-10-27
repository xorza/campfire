use std::ops::Range;

use campfire_geometry::RunChange;

use crate::brush_map::Hidden;
use crate::sight_maps::counted_maps::CountedMaps;

mod counted_maps;

/// The cells each vision group sees, and those its detectors see: each cell counts the group's
/// sights on it, so a sight that moved counts only the cells it lost and gained
/// (04-capabilities/vision.md, A stage that costs what changed). A group has a detection map only
/// once a unit of it detects.
#[derive(Debug, Default)]
pub(crate) struct SightMaps {
    /// Each group's sight, by its number.
    revealed: CountedMaps,
    /// Each group's place among the detection maps, once it has one.
    detection: Vec<Option<usize>>,
    detected: CountedMaps,
}

/// Whose sight a look counts: its group's, its detection's too when it `detects`, but on none of
/// the cells its brush hides, `hidden`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Looker<'a> {
    pub(crate) group: usize,
    pub(crate) detects: bool,
    pub(crate) hidden: Option<Hidden<'a>>,
}

impl Looker<'_> {
    /// A reveal's: its group's sight alone, brush included.
    pub(crate) const fn reveal(group: usize) -> Self {
        Self {
            group,
            detects: false,
            hidden: None,
        }
    }
}

impl SightMaps {
    /// Empty maps of `cells` cells for `groups` groups, none detecting.
    pub(crate) fn reset(&mut self, cells: usize, groups: usize) {
        self.revealed.reset(cells, groups);
        self.detection.clear();
        self.detection.resize(groups, None);
        self.detected.reset(cells, 0);
    }

    /// Counts `looker`'s sight on `cells`, a run that is not empty, once more when it `Gained`
    /// them and once less when it `Lost` them.
    #[inline]
    pub(crate) fn look(&mut self, looker: Looker<'_>, cells: Range<usize>, change: RunChange) {
        self.revealed
            .count(looker.group, cells.clone(), looker.hidden, change);
        if looker.detects {
            let map = self.detection_map(looker.group);
            self.detected.count(map, cells, looker.hidden, change);
        }
    }

    /// Counts `looker`'s sight on each of `runs`, as `look` does.
    pub(crate) fn look_runs(
        &mut self,
        looker: Looker<'_>,
        runs: &[Range<usize>],
        change: RunChange,
    ) {
        for cells in runs {
            self.look(looker, cells.clone(), change);
        }
    }

    /// The number of `group`'s detection map, which it gets on its first detector.
    fn detection_map(&mut self, group: usize) -> usize {
        if let Some(map) = self.detection[group] {
            return map;
        }
        let map = self.detected.push();
        self.detection[group] = Some(map);
        map
    }

    /// The words of the cells `group` sees.
    pub(crate) fn revealed(&self, group: usize) -> &[u64] {
        self.revealed.bits(group)
    }

    /// Whether `group` sees a cell of `cells`, a run: with its sight, or for a unit its tags
    /// hide, with its detection.
    pub(crate) fn sees(&self, group: usize, cells: Range<usize>, hidden: bool) -> bool {
        if !hidden {
            return self.revealed.any(group, cells);
        }
        self.detection[group].is_some_and(|map| self.detected.any(map, cells))
    }
}

#[cfg(test)]
mod tests;
