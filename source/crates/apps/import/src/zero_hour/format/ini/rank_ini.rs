use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The general's ranks of Zero Hour's `Rank.ini` files, as `RankInfoStore` reads them, by their
/// numbers from 1.
#[derive(Debug, Default)]
pub(crate) struct RankIni {
    ranks: BTreeMap<u32, IniRank>,
}

/// What the import reads of a rank: the skill points its player needs, `SkillPointsNeeded`, and
/// the points to buy sciences it grants, `SciencePurchasePointsGranted`, each 0 when absent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct IniRank {
    pub(crate) needed: u32,
    pub(crate) points: u32,
}

impl RankIni {
    /// Reads one INI file, adding its ranks to those read before: a rank read again keeps the
    /// fields it does not name. A rank's number that is no whole number above 0 fails the read.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).lines().blocks(BlockKinds::One("rank")) {
            let block = block?;
            let line = block.head.number;
            let number = block
                .name()?
                .parse::<u32>()
                .ok()
                .filter(|&number| number > 0)
                .ok_or(IniError::Range { line })?;
            let rank = self.ranks.entry(number).or_default();
            for line in &block.fields {
                let count = || {
                    u32::try_from(line.int()?).map_err(|_| IniError::Range { line: line.number })
                };
                match line.word() {
                    "skillpointsneeded" => rank.needed = count()?,
                    "sciencepurchasepointsgranted" => rank.points = count()?,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The ranks, by number from 1, in order.
    pub(crate) fn ranks(&self) -> impl Iterator<Item = (u32, IniRank)> + '_ {
        self.ranks.iter().map(|(&number, &rank)| (number, rank))
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\Rank.ini` of the fixture install: the game's five ranks.
    pub(crate) const RANKS: &str = "Rank 1\r
  RankName = INI:RankLevel1\r
  SkillPointsNeeded = 0\r
  SciencesGranted = SCIENCE_Rank1\r
  SciencePurchasePointsGranted = 1\r
End\r
Rank 2\r
  SkillPointsNeeded = 800 \r
  SciencePurchasePointsGranted = 1\r
End\r
Rank 3\r
  SkillPointsNeeded = 1500\r
  SciencePurchasePointsGranted = 1\r
End\r
Rank 4\r
  SkillPointsNeeded = 2500\r
  SciencePurchasePointsGranted = 1\r
End\r
Rank 5\r
  SkillPointsNeeded = 5000\r
  SciencePurchasePointsGranted = 3\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::rank_ini::samples::RANKS;

    #[test]
    fn a_rank_takes_its_number_its_points_needed_and_its_points_granted() {
        let mut ini = RankIni::default();
        ini.read(RANKS.as_bytes()).unwrap();
        let rank = |needed, points| IniRank { needed, points };
        let ranks: Vec<_> = ini.ranks().collect();
        assert_eq!(
            ranks,
            [
                (1, rank(0, 1)),
                (2, rank(800, 1)),
                (3, rank(1500, 1)),
                (4, rank(2500, 1)),
                (5, rank(5000, 3)),
            ]
        );
        // A later file keeps the fields it does not name.
        ini.read(b"Rank 5\n SciencePurchasePointsGranted = 2\nEnd\n")
            .unwrap();
        assert_eq!(ini.ranks().last(), Some((5, rank(5000, 2))));
        // A rank of number 0, of no number, or of negative points is refused.
        let fails = |text: &[u8]| RankIni::default().read(text).unwrap_err();
        assert_eq!(fails(b"Rank 0\nEnd\n"), IniError::Range { line: 1 });
        assert_eq!(fails(b"Rank first\nEnd\n"), IniError::Range { line: 1 });
        assert_eq!(
            fails(b"Rank 1\n SkillPointsNeeded = -1\nEnd\n"),
            IniError::Range { line: 2 }
        );
    }
}
