use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::stats::unit_state::UnitState;

/// A set of states: those a modifier puts its carrier in, or those a unit is in, from its
/// modifiers and its unit type. Each system asks it the one question it needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct UnitStates(u16);

impl UnitStates {
    pub(crate) fn of(states: impl IntoIterator<Item = UnitState>) -> UnitStates {
        states.into_iter().fold(UnitStates(0), UnitStates::with)
    }

    pub(crate) const fn with(self, state: UnitState) -> UnitStates {
        UnitStates(self.0 | UnitStates::bit(state))
    }

    pub(crate) const fn union(self, other: UnitStates) -> UnitStates {
        UnitStates(self.0 | other.0)
    }

    pub(crate) const fn contains(self, state: UnitState) -> bool {
        self.0 & UnitStates::bit(state) != 0
    }

    /// Whether the unit may move: not stunned, airborne or rooted.
    pub(crate) const fn can_move(self) -> bool {
        !self.any(&[UnitState::Stunned, UnitState::Airborne, UnitState::Rooted])
    }

    /// Whether the unit may attack: not stunned, airborne or disarmed.
    pub(crate) const fn can_attack(self) -> bool {
        !self.any(&[UnitState::Stunned, UnitState::Airborne, UnitState::Disarmed])
    }

    /// Whether the unit may cast: not stunned, airborne or silenced.
    pub(crate) const fn can_cast(self) -> bool {
        !self.any(&[UnitState::Stunned, UnitState::Airborne, UnitState::Silenced])
    }

    /// Whether attacks, casts and queries may choose the unit: not untargetable or
    /// invulnerable.
    pub(crate) const fn targetable(self) -> bool {
        !self.any(&[UnitState::Untargetable, UnitState::Invulnerable])
    }

    /// Whether damage reaches the unit: not invulnerable.
    pub(crate) const fn takes_damage(self) -> bool {
        !self.contains(UnitState::Invulnerable)
    }

    const fn any(self, states: &[UnitState]) -> bool {
        let mut at = 0;
        while at < states.len() {
            if self.contains(states[at]) {
                return true;
            }
            at += 1;
        }
        false
    }

    const fn bit(state: UnitState) -> u16 {
        1 << state as u16
    }
}

/// A snapshot is untrusted, so a bit of no state fails to decode.
impl<'de> Deserialize<'de> for UnitStates {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<UnitStates, D::Error> {
        let bits = u16::deserialize(deserializer)?;
        let every = UnitStates::of(UnitState::ALL).0;
        if bits & !every != 0 {
            return Err(D::Error::custom("a state bit names no state"));
        }
        Ok(UnitStates(bits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_question_is_answered_by_its_states_alone() {
        let free = UnitStates::default();
        assert!(free.can_move() && free.can_attack() && free.can_cast() && free.targetable());
        let set = |states: &[UnitState]| UnitStates::of(states.iter().copied());
        // Each state against the questions: move, attack, cast, target, damage.
        let table = [
            (UnitState::Stunned, [false, false, false, true, true]),
            (UnitState::Airborne, [false, false, false, true, true]),
            (UnitState::Rooted, [false, true, true, true, true]),
            (UnitState::Silenced, [true, true, false, true, true]),
            (UnitState::Disarmed, [true, false, true, true, true]),
            (UnitState::Untargetable, [true, true, true, false, true]),
            (UnitState::Invulnerable, [true, true, true, false, false]),
            (UnitState::SlowImmune, [true; 5]),
            (UnitState::Stealthed, [true; 5]),
            (UnitState::TrueSight, [true; 5]),
        ];
        for (state, answers) in table {
            let states = set(&[state]);
            let asked = [
                states.can_move(),
                states.can_attack(),
                states.can_cast(),
                states.targetable(),
                states.takes_damage(),
            ];
            assert_eq!(asked, answers, "{state:?}");
            assert!(states.contains(state));
        }
        let both = set(&[UnitState::Rooted]).union(set(&[UnitState::Silenced]));
        assert!(!both.can_move() && !both.can_cast() && both.can_attack());
        let decode =
            |bits: u16| postcard::from_bytes::<UnitStates>(&postcard::to_allocvec(&bits).unwrap());
        assert_eq!(decode(both.0).unwrap(), both);
        assert!(decode(1 << 10).is_err());
    }
}
