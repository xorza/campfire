use std::collections::BTreeMap;

use campfire_common::{DeclaredName, Seat};

use crate::choice_data::{ChoiceData, Offers};
use crate::error::SeatPickError;
use crate::seat_pick::SeatPick;

/// The value a session's host picked of each choice of listed values for each seat, by its place
/// among the choice's values, `NONE` where it picked none: seat by seat, each seat's choices of
/// values in the order of their names (08-sessions.md, The terms hold each seat's picks). Only its
/// constructors make one, so each place is one of its choice's, and no two seats hold one value of
/// a unique choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatPicks(Box<[u8]>);

/// A seat's pick: the seat, the place of its choice among the choices of values, in the order of
/// their names, and the place of its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Picked {
    pub(crate) seat: Seat,
    pub(crate) listed: usize,
    pub(crate) place: u8,
}

impl SeatPicks {
    /// The place of no pick.
    pub const NONE: u8 = u8::MAX;

    /// No pick of `choices` for any of `seats` seats.
    pub fn none(choices: &BTreeMap<DeclaredName, ChoiceData>, seats: usize) -> Self {
        Self(vec![Self::NONE; Self::listed(choices).count() * seats].into())
    }

    /// The picks of `values`, each of a seat of `seats` by its value, of `choices`, and no pick
    /// for the rest; an error for a seat past `seats`, a choice that lists no values, a value its
    /// choice lacks, one seat's choice picked twice, or two seats' one value of a unique choice.
    pub fn named(
        choices: &BTreeMap<DeclaredName, ChoiceData>,
        seats: usize,
        values: &[SeatPick],
    ) -> Result<Self, SeatPickError> {
        let mut picks = Self::none(choices, seats);
        let width = Self::listed(choices).count();
        for SeatPick {
            seat,
            choice,
            value,
        } in values
        {
            if seat.index() >= seats {
                return Err(SeatPickError::Seat(*seat));
            }
            let (at, data) = Self::listed(choices)
                .enumerate()
                .find(|(_, (name, _))| *name == choice)
                .map(|(at, (_, data))| (at, data))
                .ok_or_else(|| SeatPickError::Unknown(choice.clone()))?;
            let place = data
                .values
                .iter()
                .position(|held| held == value)
                .ok_or_else(|| SeatPickError::NoValue {
                    choice: choice.clone(),
                    value: value.clone(),
                })?;
            let held = &mut picks.0[seat.index() * width + at];
            if *held != Self::NONE {
                return Err(SeatPickError::Twice {
                    seat: *seat,
                    choice: choice.clone(),
                });
            }
            *held = u8::try_from(place).expect("a choice lists fewer values than a byte's");
        }
        Self::checked(choices, seats, &picks.0)
    }

    /// `places`, as a session's terms carry them, the picks of `choices` for `seats` seats; an
    /// error for another count than the seats' choices of values, a place past its choice's
    /// values, or two seats' one value of a unique choice.
    pub fn checked(
        choices: &BTreeMap<DeclaredName, ChoiceData>,
        seats: usize,
        places: &[u8],
    ) -> Result<Self, SeatPickError> {
        let width = Self::listed(choices).count();
        let expected = width * seats;
        if places.len() != expected {
            return Err(SeatPickError::Count {
                given: places.len(),
                expected,
            });
        }
        for (at, (name, data)) in Self::listed(choices).enumerate() {
            let column = places.iter().skip(at).step_by(width);
            let mut picked = Vec::with_capacity(seats);
            for (seat, &place) in (0..).map(Seat::new).zip(column) {
                if place == Self::NONE {
                    continue;
                }
                if usize::from(place) >= data.values.len() {
                    let choice = name.clone();
                    return Err(SeatPickError::Place {
                        seat,
                        choice,
                        place,
                    });
                }
                if data.unique && picked.contains(&place) {
                    let choice = name.clone();
                    return Err(SeatPickError::Taken { choice, place });
                }
                picked.push(place);
            }
        }
        Ok(Self(places.into()))
    }

    /// The places, as a session's terms carry them.
    pub fn places(&self) -> &[u8] {
        &self.0
    }

    /// Each pick of `choices`, the ones it was made of, seat by seat, each seat's in the order of
    /// the choices' names.
    pub(crate) fn picked(
        &self,
        choices: &BTreeMap<DeclaredName, ChoiceData>,
    ) -> impl Iterator<Item = Picked> {
        let width = Self::listed(choices).count();
        let picked = self.0.iter().enumerate();
        picked
            .filter(|&(_, &place)| place != Self::NONE)
            .map(move |(at, &place)| Picked {
                seat: Seat::new(u32::try_from(at / width).expect("a seat fits u32")),
                listed: at % width,
                place,
            })
    }

    /// The choices of listed values, in the order of their names.
    fn listed(
        choices: &BTreeMap<DeclaredName, ChoiceData>,
    ) -> impl Iterator<Item = (&DeclaredName, &ChoiceData)> {
        choices
            .iter()
            .filter(|(_, choice)| choice.offers == Offers::Values)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn seat_picks_are_a_place_of_each_choice_of_values_seat_by_seat() {
        let choice = |text: &str| Toml::parse::<ChoiceData>(text).unwrap();
        let name = |name: &str| DeclaredName::new(name).unwrap();
        // A faction of three values, a unique color of two, and a hero of avatars, which takes
        // no seat pick.
        let choices: BTreeMap<DeclaredName, ChoiceData> = [
            (
                "faction",
                choice("name = \"f\"\noffers = \"values\"\nvalues = [\"america\", \"china\", \"gla\"]"),
            ),
            (
                "color",
                choice("name = \"c\"\noffers = \"values\"\nvalues = [\"red\", \"blue\"]\nunique = true"),
            ),
            ("hero", choice("name = \"h\"\noffers = \"avatars\"")),
        ]
        .map(|(key, choice)| (name(key), choice))
        .into();
        // Two seats, each its color then its faction, by name.
        let none = SeatPicks::none(&choices, 2);
        assert_eq!(none.places(), [SeatPicks::NONE; 4]);
        assert_eq!(none.picked(&choices).count(), 0);
        let pick = |seat, choice: &str, value: &str| SeatPick {
            seat: Seat::new(seat),
            choice: name(choice),
            value: name(value),
        };
        let picks = SeatPicks::named(
            &choices,
            2,
            &[pick(1, "faction", "gla"), pick(0, "color", "blue")],
        )
        .unwrap();
        let n = SeatPicks::NONE;
        assert_eq!(picks.places(), [1, n, n, 2]);
        let picked: Vec<(u32, usize, u8)> = picks
            .picked(&choices)
            .map(|picked| (picked.seat.get(), picked.listed, picked.place))
            .collect();
        assert_eq!(picked, [(0, 0, 1), (1, 1, 2)]);
        assert_eq!(SeatPicks::checked(&choices, 2, picks.places()), Ok(picks));
        // Refused by name: a seat past the session's, a choice of avatars or none, a value the
        // choice lacks, a choice picked twice, and one value of a unique choice for two seats.
        let refused = |values: &[SeatPick]| SeatPicks::named(&choices, 2, values).unwrap_err();
        assert_eq!(
            refused(&[pick(2, "faction", "gla")]),
            SeatPickError::Seat(Seat::new(2))
        );
        assert_eq!(
            refused(&[pick(0, "hero", "husk")]),
            SeatPickError::Unknown(name("hero"))
        );
        assert_eq!(
            refused(&[pick(0, "faction", "usa")]),
            SeatPickError::NoValue {
                choice: name("faction"),
                value: name("usa")
            }
        );
        assert_eq!(
            refused(&[pick(0, "faction", "gla"), pick(0, "faction", "china")]),
            SeatPickError::Twice {
                seat: Seat::new(0),
                choice: name("faction")
            }
        );
        assert_eq!(
            refused(&[pick(0, "color", "red"), pick(1, "color", "red")]),
            SeatPickError::Taken {
                choice: name("color"),
                place: 0
            }
        );
        // Two seats may pick one value of a choice that is not unique.
        let both = [pick(0, "faction", "china"), pick(1, "faction", "china")];
        assert_eq!(
            SeatPicks::named(&choices, 2, &both).unwrap().places(),
            [n, 1, n, 1]
        );
        // Refused as the terms carry them: another count, and a place past a choice's values.
        assert_eq!(
            SeatPicks::checked(&choices, 2, &[n; 3]),
            Err(SeatPickError::Count {
                given: 3,
                expected: 4
            })
        );
        assert_eq!(
            SeatPicks::checked(&choices, 2, &[n, 3, n, n]),
            Err(SeatPickError::Place {
                seat: Seat::new(0),
                choice: name("faction"),
                place: 3
            })
        );
        // A mode of no choice of values takes no pick.
        let none = BTreeMap::new();
        assert!(SeatPicks::checked(&none, 2, &[]).is_ok());
    }
}
