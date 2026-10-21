use super::*;

#[test]
fn a_seat_pick_reads_as_a_seat_a_choice_and_a_value() {
    let read = |text: &str| text.parse::<SeatPick>();
    let name = |name: &str| DeclaredName::new(name).unwrap();
    assert_eq!(
        read("1:faction=china"),
        Ok(SeatPick {
            seat: Seat::new(1),
            choice: name("faction"),
            value: name("china"),
        })
    );
    assert_eq!(read("faction=china"), Err(SeatPickTextError::Form));
    assert_eq!(read("1:faction"), Err(SeatPickTextError::Form));
    assert_eq!(
        read("one:faction=china"),
        Err(SeatPickTextError::Seat("one".to_owned()))
    );
    assert_eq!(
        read("-1:faction=china"),
        Err(SeatPickTextError::Seat("-1".to_owned()))
    );
    assert_eq!(
        read("0:Faction=china"),
        Err(SeatPickTextError::Choice("Faction".to_owned()))
    );
    assert_eq!(
        read("0:faction=China"),
        Err(SeatPickTextError::Value("China".to_owned()))
    );
}
