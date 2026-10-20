use std::str::FromStr;

use campfire_common::{DeclaredName, Seat};

use crate::error::SeatPickTextError;

/// A host's pick of a seat's choice by its value, as `--pick` writes it: `seat:choice=value`
/// (08-sessions.md, The terms hold each seat's picks). The mode's choices check it as a
/// session opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatPick {
    pub seat: Seat,
    pub choice: DeclaredName,
    pub value: DeclaredName,
}

impl FromStr for SeatPick {
    type Err = SeatPickTextError;

    fn from_str(text: &str) -> Result<Self, SeatPickTextError> {
        let (seat, rest) = text.split_once(':').ok_or(SeatPickTextError::Form)?;
        let (choice, value) = rest.split_once('=').ok_or(SeatPickTextError::Form)?;
        let seat = seat
            .parse()
            .map(Seat::new)
            .map_err(|_| SeatPickTextError::Seat(seat.to_owned()))?;
        let choice = DeclaredName::new(choice)
            .ok_or_else(|| SeatPickTextError::Choice(choice.to_owned()))?;
        let value =
            DeclaredName::new(value).ok_or_else(|| SeatPickTextError::Value(value.to_owned()))?;
        Ok(Self {
            seat,
            choice,
            value,
        })
    }
}

#[cfg(test)]
mod tests {
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
}
