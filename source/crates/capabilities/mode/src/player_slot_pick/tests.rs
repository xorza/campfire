use super::*;

#[test]
fn a_player_slot_pick_reads_as_a_player_slot_a_choice_and_a_value() {
    let read = |text: &str| text.parse::<PlayerSlotPick>();
    let name = |name: &str| DeclaredName::new(name).unwrap();
    assert_eq!(
        read("1:faction=china"),
        Ok(PlayerSlotPick {
            player_slot: PlayerSlot::new(1),
            choice: name("faction"),
            value: name("china"),
        })
    );
    assert_eq!(read("faction=china"), Err(PlayerSlotPickTextError::Form));
    assert_eq!(read("1:faction"), Err(PlayerSlotPickTextError::Form));
    assert_eq!(
        read("one:faction=china"),
        Err(PlayerSlotPickTextError::PlayerSlot("one".to_owned()))
    );
    assert_eq!(
        read("-1:faction=china"),
        Err(PlayerSlotPickTextError::PlayerSlot("-1".to_owned()))
    );
    assert_eq!(
        read("0:Faction=china"),
        Err(PlayerSlotPickTextError::Choice("Faction".to_owned()))
    );
    assert_eq!(
        read("0:faction=China"),
        Err(PlayerSlotPickTextError::Value("China".to_owned()))
    );
}
