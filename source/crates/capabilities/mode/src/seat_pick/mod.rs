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
mod tests;
