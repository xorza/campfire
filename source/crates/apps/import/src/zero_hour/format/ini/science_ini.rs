use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The sciences of Zero Hour's `Science.ini` files, as `ScienceStore` reads them, by their names,
/// of their case.
#[derive(Debug, Default)]
pub(crate) struct ScienceIni {
    sciences: BTreeMap<String, IniScience>,
}

/// What the import reads of a science: the sciences it needs, `PrerequisiteSciences`, none for
/// `None`, and its cost in points, `SciencePurchasePointCost`, 0 for one no player buys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IniScience {
    pub(crate) prerequisites: Vec<String>,
    pub(crate) cost: u32,
}

impl ScienceIni {
    /// Reads one INI file, adding its sciences to those read before: a science read again keeps
    /// the fields it does not name.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("science"))
        {
            let block = block?;
            let science = self.sciences.entry(block.name()?.to_owned()).or_default();
            for line in &block.fields {
                match line.word() {
                    "prerequisitesciences" => {
                        let named = line.tokens[1..].iter().copied();
                        let named = named.filter(|name| !name.eq_ignore_ascii_case("none"));
                        science.prerequisites = named.map(str::to_owned).collect();
                    }
                    "sciencepurchasepointcost" => {
                        science.cost = u32::try_from(line.int()?)
                            .map_err(|_| IniError::Range { line: line.number })?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The science `name`, of its case; none for one no file defines.
    pub(crate) fn named(&self, name: &str) -> Option<&IniScience> {
        self.sciences.get(name)
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\Science.ini` of the fixture install: America's science, the first three ranks',
    /// and the first two levels of the paradrop.
    pub(crate) const SCIENCES: &str = "Science SCIENCE_AMERICA\r
  PrerequisiteSciences = None\r
  SciencePurchasePointCost = 0  ; not purchasable, not free\r
  IsGrantable = No\r
End\r
Science SCIENCE_Rank1\r
  PrerequisiteSciences = None\r
  SciencePurchasePointCost = 0\r
End\r
Science SCIENCE_Rank2\r
  PrerequisiteSciences = SCIENCE_Rank1\r
  SciencePurchasePointCost = 0\r
End\r
Science SCIENCE_Rank3\r
  PrerequisiteSciences = SCIENCE_Rank2\r
  SciencePurchasePointCost = 0\r
End\r
Science SCIENCE_Paradrop1\r
  PrerequisiteSciences = SCIENCE_AMERICA SCIENCE_Rank3\r
  SciencePurchasePointCost = 1\r
  IsGrantable = Yes\r
End\r
Science SCIENCE_Paradrop2\r
  PrerequisiteSciences = SCIENCE_Paradrop1 SCIENCE_Rank3\r
  SciencePurchasePointCost = 1\r
  IsGrantable = Yes\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::science_ini::samples::SCIENCES;

    #[test]
    fn a_science_takes_its_prerequisites_and_its_cost() {
        let mut ini = ScienceIni::default();
        ini.read(SCIENCES.as_bytes()).unwrap();
        let science = |prerequisites: &[&str], cost| IniScience {
            prerequisites: prerequisites.iter().map(|&name| name.to_owned()).collect(),
            cost,
        };
        assert_eq!(ini.named("SCIENCE_AMERICA"), Some(&science(&[], 0)));
        assert_eq!(
            ini.named("SCIENCE_Paradrop2"),
            Some(&science(&["SCIENCE_Paradrop1", "SCIENCE_Rank3"], 1))
        );
        assert_eq!(ini.named("science_paradrop2"), None);
        // A later file keeps the fields it does not name.
        ini.read(b"Science SCIENCE_Paradrop2\n SciencePurchasePointCost = 2\nEnd\n")
            .unwrap();
        assert_eq!(
            ini.named("SCIENCE_Paradrop2"),
            Some(&science(&["SCIENCE_Paradrop1", "SCIENCE_Rank3"], 2))
        );
        // A negative cost is refused.
        let wrong = b"Science A\n SciencePurchasePointCost = -1\nEnd\n";
        assert_eq!(
            ScienceIni::default().read(wrong).unwrap_err(),
            IniError::Range { line: 2 }
        );
    }
}
