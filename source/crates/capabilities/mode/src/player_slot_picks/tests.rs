use campfire_codec::Toml;

use super::*;

#[test]
fn player_slot_picks_are_a_place_of_each_choice_of_values_player_slot_by_player_slot() {
    let choice = |text: &str| Toml::parse::<ChoiceData>(text).unwrap();
    let name = |name: &str| DeclaredName::new(name).unwrap();
    // A faction of three values, a unique color of two, and a hero of avatars, which takes
    // no player slot pick.
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
    // Two player slots, each its color then its faction, by name.
    let none = PlayerSlotPicks::none(&choices, 2);
    assert_eq!(none.places(), [PlayerSlotPicks::NONE; 4]);
    assert_eq!(none.picked(&choices).count(), 0);
    let pick = |player_slot, choice: &str, value: &str| PlayerSlotPick {
        player_slot: PlayerSlot::new(player_slot),
        choice: name(choice),
        value: name(value),
    };
    let picks = PlayerSlotPicks::named(
        &choices,
        2,
        &[pick(1, "faction", "gla"), pick(0, "color", "blue")],
    )
    .unwrap();
    let n = PlayerSlotPicks::NONE;
    assert_eq!(picks.places(), [1, n, n, 2]);
    let picked: Vec<(u32, usize, u8)> = picks
        .picked(&choices)
        .map(|picked| (picked.player_slot.get(), picked.listed, picked.place))
        .collect();
    assert_eq!(picked, [(0, 0, 1), (1, 1, 2)]);
    // Player slot 1's faction is GLA, player slot 0's none; its color blue; a choice of avatars, a
    // choice the mode lacks and a player slot past the picks give none.
    let value = |player_slot, choice: &str| {
        picks
            .value(&choices, PlayerSlot::new(player_slot), &name(choice))
            .cloned()
    };
    assert_eq!(
        [value(1, "faction"), value(0, "faction"), value(0, "color")],
        [Some(name("gla")), None, Some(name("blue"))]
    );
    assert_eq!(
        [value(0, "hero"), value(0, "side"), value(2, "faction")],
        [None, None, None]
    );
    assert_eq!(
        PlayerSlotPicks::checked(&choices, 2, picks.places()),
        Ok(picks.clone())
    );
    // Of the places alone, two whole rows of two choices are two player slots; three places are
    // no whole rows.
    assert_eq!(
        PlayerSlotPicks::of_places(&choices, picks.places()),
        Ok(picks.clone())
    );
    assert!(PlayerSlotPicks::of_places(&choices, &[1, n, n]).is_err());
    // Refused by name: a player slot past the session's, a choice of avatars or none, a value the
    // choice lacks, a choice picked twice, and one value of a unique choice for two player slots.
    let refused =
        |values: &[PlayerSlotPick]| PlayerSlotPicks::named(&choices, 2, values).unwrap_err();
    assert_eq!(
        refused(&[pick(2, "faction", "gla")]),
        PlayerSlotPickError::PlayerSlot(PlayerSlot::new(2))
    );
    assert_eq!(
        refused(&[pick(0, "hero", "husk")]),
        PlayerSlotPickError::Unknown(name("hero"))
    );
    assert_eq!(
        refused(&[pick(0, "faction", "usa")]),
        PlayerSlotPickError::NoValue {
            choice: name("faction"),
            value: name("usa")
        }
    );
    assert_eq!(
        refused(&[pick(0, "faction", "gla"), pick(0, "faction", "china")]),
        PlayerSlotPickError::Twice {
            player_slot: PlayerSlot::new(0),
            choice: name("faction")
        }
    );
    assert_eq!(
        refused(&[pick(0, "color", "red"), pick(1, "color", "red")]),
        PlayerSlotPickError::Taken {
            choice: name("color"),
            place: 0
        }
    );
    // Two player slots may pick one value of a choice that is not unique.
    let both = [pick(0, "faction", "china"), pick(1, "faction", "china")];
    assert_eq!(
        PlayerSlotPicks::named(&choices, 2, &both).unwrap().places(),
        [n, 1, n, 1]
    );
    // Refused as the terms carry them: another count, and a place past a choice's values.
    assert_eq!(
        PlayerSlotPicks::checked(&choices, 2, &[n; 3]),
        Err(PlayerSlotPickError::Count {
            given: 3,
            expected: 4
        })
    );
    assert_eq!(
        PlayerSlotPicks::checked(&choices, 2, &[n, 3, n, n]),
        Err(PlayerSlotPickError::Place {
            player_slot: PlayerSlot::new(0),
            choice: name("faction"),
            place: 3
        })
    );
    // A mode of no choice of values takes no pick.
    let none = BTreeMap::new();
    assert!(PlayerSlotPicks::checked(&none, 2, &[]).is_ok());
}
