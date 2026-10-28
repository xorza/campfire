use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniText;

/// The skill sets of Zero Hour's `AIData.ini` files, as `AI::parseSideInfo` reads them: each side's
/// sets of sciences a skirmish AI buys, by the side's name, of its case, each set in the order of
/// its number and its sciences in their order; and the values of a human player's retaliation,
/// each its last line's.
#[derive(Debug, Default)]
pub(crate) struct AiDataIni {
    sides: BTreeMap<String, BTreeMap<u8, Vec<String>>>,
    pub(crate) retaliation: AiRetaliation,
}

/// What `AIData` gives a human player's retaliation (`TAiData`): its `MaxRetaliationDistance`,
/// its `RetaliationFriendsRadius`, its `GuardChaseUnitsDuration` in milliseconds, and its
/// `GuardInnerModifierHuman` and `GuardOuterModifierHuman`, the vision range's factors of a
/// retaliator's leash and of its target's reach past it; each none when no file gives it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct AiRetaliation {
    pub(crate) distance: Option<f32>,
    pub(crate) radius: Option<f32>,
    pub(crate) chase_ms: Option<i32>,
    pub(crate) inner: Option<f32>,
    pub(crate) outer: Option<f32>,
}

/// The block a line of an `AIData` file is inside.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Inside {
    /// `AIData`.
    AiData,
    /// `SideInfo <side>`.
    Side(String),
    /// `SkillSet<n>` of a side.
    Set { side: String, number: u8 },
    /// A `SkirmishBuildList` and its `Structure`s, whose lines the import does not read.
    BuildList,
}

impl AiDataIni {
    /// `SkillSet1` to `SkillSet5`, `MAX_SKILL_SETS`.
    const SETS: u8 = 5;

    /// Reads one INI file, adding its sides to those read before: a set read again starts its
    /// sciences again, as `AI::parseSkillSet` clears it. A file of another block, a set of a
    /// number past 1 to 5, a stray `End` and a file that ends inside a block are refused.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let lines = text.lines();
        let mut stack: Vec<Inside> = Vec::new();
        for line in lines.iter() {
            let (word, number) = (line.word(), line.number);
            if word == "end" {
                stack.pop().ok_or(IniError::StrayEnd { line: number })?;
                continue;
            }
            let opened = match (stack.last(), word) {
                (None, "aidata") => Some(Inside::AiData),
                (None, _) => return Err(IniError::NotBlock { line: number }),
                (Some(Inside::AiData), "sideinfo") => Some(Inside::Side(line.value()?.to_owned())),
                (Some(Inside::AiData), "skirmishbuildlist") => Some(Inside::BuildList),
                (Some(Inside::AiData), field) => {
                    let retaliation = &mut self.retaliation;
                    match field {
                        "maxretaliationdistance" => retaliation.distance = Some(line.real()?),
                        "retaliationfriendsradius" => retaliation.radius = Some(line.real()?),
                        "guardchaseunitsduration" => retaliation.chase_ms = Some(line.int()?),
                        "guardinnermodifierhuman" => retaliation.inner = Some(line.real()?),
                        "guardoutermodifierhuman" => retaliation.outer = Some(line.real()?),
                        _ => {}
                    }
                    None
                }
                (Some(Inside::BuildList), "structure") => Some(Inside::BuildList),
                (Some(Inside::Side(side)), _) if word.starts_with("skillset") => {
                    let set = word["skillset".len()..]
                        .parse::<u8>()
                        .ok()
                        .filter(|set| (1..=Self::SETS).contains(set))
                        .ok_or(IniError::NotListed { line: number })?;
                    let side = side.clone();
                    self.sides
                        .entry(side.clone())
                        .or_default()
                        .insert(set, Vec::new());
                    Some(Inside::Set { side, number: set })
                }
                (Some(Inside::Set { side, number: set }), "science") => {
                    let sets = self.sides.get_mut(side).expect("a set's side is read");
                    let held = sets.get_mut(set).expect("a set is read as it opens");
                    held.push(line.value()?.to_owned());
                    None
                }
                _ => None,
            };
            stack.extend(opened);
        }
        if !stack.is_empty() {
            return Err(IniError::Unclosed);
        }
        Ok(())
    }

    /// The skill sets of `side`, in the order of their numbers, each of its sciences; none for a
    /// side of none.
    pub(crate) fn sets(&self, side: &str) -> impl Iterator<Item = &[String]> {
        self.sides
            .get(side)
            .into_iter()
            .flat_map(|sets| sets.values().map(Vec::as_slice))
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\Default\AIData.ini` of the fixture install: the values of retaliation,
    /// America's side, its build list and its two skill sets.
    pub(crate) const AI_DATA: &str = "AIData\r
  TeamSeconds = 10\r
  GuardInnerModifierHuman = 1.8 ;1.1\r
  GuardOuterModifierHuman = 2.2\r
  GuardChaseUnitsDuration = 10000\r
  MaxRetaliationDistance = 200.0\r
  RetaliationFriendsRadius = 120.0\r
  SideInfo America\r
    ResourceGatherersEasy = 2\r
    SkillSet1\r
      Science = SCIENCE_Paradrop1\r
      Science = SCIENCE_Paradrop2\r
    End\r
    SkillSet2\r
      Science = SCIENCE_Paradrop2\r
    End\r
  End\r
  SkirmishBuildList America\r
    Structure AmericaCommandCenter\r
      Location = X:0 Y:0\r
    End\r
  End\r
END\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::ai_data_ini::samples::AI_DATA;

    #[test]
    fn a_side_takes_its_skill_sets_in_order_each_of_its_sciences() {
        let mut ini = AiDataIni::default();
        ini.read(AI_DATA.as_bytes()).unwrap();
        let sets: Vec<&[String]> = ini.sets("America").collect();
        assert_eq!(
            sets,
            [
                &[
                    "SCIENCE_Paradrop1".to_owned(),
                    "SCIENCE_Paradrop2".to_owned()
                ][..],
                &["SCIENCE_Paradrop2".to_owned()][..],
            ]
        );
        assert_eq!(ini.sets("China").count(), 0);
        // The retaliation's values, each its line's.
        assert_eq!(
            ini.retaliation,
            AiRetaliation {
                distance: Some(200.0),
                radius: Some(120.0),
                chase_ms: Some(10_000),
                inner: Some(1.8),
                outer: Some(2.2),
            }
        );
        // A set read again starts again.
        ini.read(b"AIData\n SideInfo America\n  SkillSet1\n   Science = A\n  End\n End\nEnd\n")
            .unwrap();
        assert_eq!(ini.sets("America").next(), Some(&["A".to_owned()][..]));
        let fails = |text: &[u8]| AiDataIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"AIData\n SideInfo A\n  SkillSet6\n  End\n End\nEnd\n"),
            IniError::NotListed { line: 3 }
        );
        assert_eq!(fails(b"Weapon A\nEnd\n"), IniError::NotBlock { line: 1 });
        assert_eq!(fails(b"AIData\nEnd\nEnd\n"), IniError::StrayEnd { line: 3 });
        assert_eq!(fails(b"AIData\n SideInfo A\n"), IniError::Unclosed);
    }
}
