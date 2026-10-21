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
            choice(
                "name = \"c\"\noffers = \"values\"\nvalues = [\"red\", \"blue\"]\nunique = true",
            ),
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
