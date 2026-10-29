use std::collections::BTreeMap;

use campfire_client_files::{ConditionTest, StateSet};
use campfire_common::DeclaredName;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::ini::object_ini::draw_module::DrawModule;
use crate::zero_hour::look_flags::{FlagCondition, LookFlags};

/// The sets of a draw module's states, each state's in order, each of the names of its tested
/// conditions and the count of the others, less each set another beats whatever its unit holds
/// (`StateSet::beats`), which no unit draws, as Zero Hour's `SparseMatchFinder::findBestInfoSlow`
/// never picks it; and the test of each name they hold (02-engine-core.md, A set counts the
/// conditions no test says). A state of no set left is never drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DrawSets {
    pub(crate) states: Vec<Vec<StateSet>>,
    pub(crate) tests: BTreeMap<DeclaredName, ConditionTest>,
}

impl DrawSets {
    /// The sets of `draw`'s states, of the conditions `flags` gives its flags; an error for a
    /// flag that is no declared name.
    pub(crate) fn of(draw: &DrawModule, flags: &LookFlags) -> Result<Self, ZeroHourError> {
        let mut tests = BTreeMap::new();
        let mut states = Vec::new();
        for state in draw.states() {
            let mut sets = Vec::with_capacity(state.sets.len());
            for set in state.sets {
                let mut names = Vec::with_capacity(set.flags().len());
                let mut untested = 0;
                for flag in set.flags() {
                    let FlagCondition { name, test } = flags.condition(flag)?;
                    match test {
                        Some(test) => {
                            tests.insert(name.clone(), test);
                            names.push(name);
                        }
                        None => untested += 1,
                    }
                }
                sets.push(StateSet::new(names, untested));
            }
            states.push(sets);
        }
        Self::trim(&mut states);
        let held = states.iter().flatten().flat_map(StateSet::names);
        let held: Vec<&DeclaredName> = held.collect();
        tests.retain(|name, _| held.contains(&name));
        Ok(Self { states, tests })
    }

    /// Drops each set of `states` another of them beats whatever its unit holds.
    fn trim(states: &mut [Vec<StateSet>]) {
        let sets: Vec<&StateSet> = states.iter().flatten().collect();
        #[expect(
            clippy::needless_collect,
            reason = "the marks outlive the sets' borrow, which the retain takes mutably"
        )]
        let kept: Vec<bool> = (sets.iter().enumerate())
            .map(|(at, set)| {
                let others = sets.iter().enumerate().filter(|&(by, _)| by != at);
                !others
                    .into_iter()
                    .any(|(by, other)| other.beats(set, by < at))
            })
            .collect();
        let mut kept = kept.into_iter();
        for sets in states {
            sets.retain(|_| kept.next().expect("a mark for each set"));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::zero_hour::format::ini::game_data_ini::GameDataIni;
    use crate::zero_hour::format::ini::object_ini::ObjectIni;
    use crate::zero_hour::rules_names::RulesNames;

    use super::*;

    #[test]
    fn a_set_counts_its_untested_conditions_and_a_beaten_set_goes() {
        // A default; `DAMAGED`; `DAMAGED GARRISONED`, beaten by `DAMAGED`, of as many in common
        // and one fewer lacked, so its state goes; `MOVING SOLD`, which none beats, as no other
        // set holds `MOVING`; and `SOLD` alone, which the default beats, coming first.
        let text = "Object Tank\n  Draw = W3DModelDraw ModuleTag_01\n\
                    DefaultConditionState\n  Model = TANK\nEnd\n\
                    ConditionState = DAMAGED\n  Model = TANK_D\nEnd\n\
                    ConditionState = DAMAGED GARRISONED\n  Model = TANK_G\nEnd\n\
                    ConditionState = MOVING SOLD\n  Model = TANK_M\nEnd\n\
                    ConditionState = SOLD\n  Model = TANK_S\nEnd\n  End\nEnd\n";
        let objects = ObjectIni::of(&[text]);
        let mut game_data = GameDataIni::default();
        game_data.damaged_threshold = 0.5;
        game_data.really_damaged_threshold = 0.25;
        let flags = LookFlags::of(&game_data, &RulesNames::default(), &objects).unwrap();
        let tank = objects.object(b"Tank").unwrap();
        let sets = DrawSets::of(&tank.draws()[0], &flags).unwrap();
        let name = |name: &str| DeclaredName::new(name).unwrap();
        assert_eq!(
            sets.states,
            [
                vec![StateSet::default()],
                vec![StateSet::from([name("damaged")])],
                Vec::new(),
                vec![StateSet::new([name("moving")], 1)],
                Vec::new(),
            ]
        );
        // The tests of the names the sets keep, none of the untested.
        let names: Vec<&str> = sets.tests.keys().map(DeclaredName::as_str).collect();
        assert_eq!(names, ["damaged", "moving"]);
    }
}
