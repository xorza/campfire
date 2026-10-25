use campfire_common::Tick;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// How long a modifier's instance lasts: while any hold keeps it, and while an application of
/// its own keeps it, until its end if it has one. It ends only when neither keeps it, so a timed
/// application over a held instance, or a hold over a timed one, keeps both lifetimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lifetime {
    pub(crate) holds: HoldSet,
    /// The application that keeps it of its own, and its end; none while only holds keep it.
    pub(crate) applied: Option<Ends>,
}

/// When an application of its own stops keeping an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ends {
    /// It lasts until it is removed.
    Never,
    /// The first tick it no longer holds.
    At(Tick),
}

/// What holds an instance beside an application: its ability's passive, which its rank keeps; an
/// aura or an area, which hold it while their rules select its carrier; an action's `hold`,
/// which its toggle that is on or its channel that runs keeps; its carrier's player, which holds
/// it while it holds the modifier and its `affects` selects the carrier; its carrier's shot
/// streak, which its stage keeps; an item its carrier carries; or its carrier's type's upgrade,
/// which keeps it while its triggers are met. Every hold from one source of
/// one modifier is one hold: which of them holds it changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    Passive,
    Held,
    Running,
    Player,
    Streak,
    Item,
    Upgrade,
}

impl Hold {
    /// Whether it holds as a passive does, through a death and with no stack: a unit's or an
    /// action's passive, or an item's.
    pub const fn is_passive(self) -> bool {
        matches!(self, Self::Passive | Self::Item)
    }
}

/// The holds that keep an instance, a bit each.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct HoldSet(u8);

impl Lifetime {
    /// The lifetime of an application by `hold`, which keeps it while it holds.
    pub const fn held(hold: Hold) -> Self {
        Self {
            holds: HoldSet::of(hold),
            applied: None,
        }
    }

    /// The lifetime of an application of its own, until `ends`.
    pub const fn applied(ends: Ends) -> Self {
        Self {
            holds: HoldSet::NONE,
            applied: Some(ends),
        }
    }

    /// The lifetime once `new`, a later application of the same modifier from the same source,
    /// joins it: its holds join, and an application of its own replaces this one's, as a
    /// refresh takes the new end; a hold leaves this one's.
    pub(crate) fn joined(self, new: Self) -> Self {
        Self {
            holds: self.holds.union(new.holds),
            applied: new.applied.or(self.applied),
        }
    }

    /// The lifetime once the holds of `new`, a later application of the same modifier from the
    /// same source that changes nothing else, join it.
    pub(crate) const fn held_too(self, new: Self) -> Self {
        Self {
            holds: self.holds.union(new.holds),
            applied: self.applied,
        }
    }

    /// Whether `hold` keeps it.
    pub const fn held_by(self, hold: Hold) -> bool {
        self.holds.contains(hold)
    }

    /// Whether a passive's hold keeps it, which keeps it through a death and with no stack.
    pub const fn held_as_passive(self) -> bool {
        self.held_by(Hold::Passive) || self.held_by(Hold::Item)
    }

    /// Its end: none while a hold keeps it or its application never ends.
    pub const fn until(self) -> Option<Tick> {
        match (self.holds.is_empty(), self.applied) {
            (true, Some(Ends::At(until))) => Some(until),
            _ => None,
        }
    }

    /// The lifetime as tick `now` starts: an application whose end it reached keeps it no more.
    /// Whether anything still keeps it.
    pub(crate) fn lasts(&mut self, now: Tick) -> bool {
        if let Some(Ends::At(until)) = self.applied
            && until <= now
        {
            self.applied = None;
        }
        !self.holds.is_empty() || self.applied.is_some()
    }

    /// Lets go of `hold`; whether anything still keeps it.
    pub(crate) const fn release(&mut self, hold: Hold) -> bool {
        self.holds = self.holds.without(hold);
        !self.holds.is_empty() || self.applied.is_some()
    }
}

impl HoldSet {
    const NONE: Self = Self(0);
    const ALL: u8 = Self::bit(Hold::Passive)
        | Self::bit(Hold::Held)
        | Self::bit(Hold::Running)
        | Self::bit(Hold::Player)
        | Self::bit(Hold::Streak)
        | Self::bit(Hold::Item)
        | Self::bit(Hold::Upgrade);

    pub(crate) const fn of(hold: Hold) -> Self {
        Self(Self::bit(hold))
    }

    const fn without(self, hold: Hold) -> Self {
        Self(self.0 & !Self::bit(hold))
    }

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(crate) const fn contains(self, hold: Hold) -> bool {
        self.0 & Self::bit(hold) != 0
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.0 == 0
    }

    const fn bit(hold: Hold) -> u8 {
        1 << hold as u8
    }
}

/// A snapshot is untrusted, so a bit of no hold fails to decode.
impl<'de> Deserialize<'de> for HoldSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bits = u8::deserialize(deserializer)?;
        if bits & !Self::ALL != 0 {
            return Err(D::Error::custom("a bit of no hold"));
        }
        Ok(Self(bits))
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn an_instance_lasts_while_a_hold_or_its_own_application_keeps_it() {
        let at = |tick| Ends::At(Tick::new(tick));
        // Held, then applied until tick 5: the aura's leaving keeps it until 5, and tick 5 ends it.
        let held = Lifetime::held(Hold::Held).joined(Lifetime::applied(at(5)));
        let mut later = held;
        assert_eq!((held.until(), later.lasts(Tick::new(9))), (None, true));
        let mut released = held;
        assert!(released.release(Hold::Held));
        assert_eq!(released.until(), Some(Tick::new(5)));
        assert!(released.lasts(Tick::new(4)) && !released.lasts(Tick::new(5)));
        // Applied until tick 5, then held: past tick 5 the hold keeps it; released, it ends.
        let mut timed = Lifetime::applied(at(5)).joined(Lifetime::held(Hold::Held));
        assert!(timed.lasts(Tick::new(7)));
        assert!(!timed.release(Hold::Held));
        // An application that never ends outlasts its holds; a refresh takes the new end.
        let mut kept = Lifetime::applied(Ends::Never).joined(Lifetime::held(Hold::Passive));
        assert!(kept.release(Hold::Passive) && kept.lasts(Tick::new(1_000)));
        let refreshed = Lifetime::applied(at(3)).joined(Lifetime::applied(at(8)));
        assert_eq!(refreshed.until(), Some(Tick::new(8)));
        // A passive and a hold are two holds, each released alone.
        let mut both = Lifetime::held(Hold::Passive).held_too(Lifetime::held(Hold::Held));
        assert!(both.release(Hold::Held) && both.held_by(Hold::Passive));
        assert!(!both.release(Hold::Passive));
        // An item's hold and a passive are two holds too, and either holds as a passive does.
        let mut both = Lifetime::held(Hold::Passive).held_too(Lifetime::held(Hold::Item));
        assert!(both.release(Hold::Passive) && both.held_as_passive());
        assert!(!both.release(Hold::Item) && !both.held_as_passive());
        assert!(!Lifetime::held(Hold::Held).held_as_passive());
    }

    #[test]
    fn a_hold_set_decodes_only_bits_of_holds() {
        let decode = |bits: u8| Binary::decode::<HoldSet>(&[bits]).ok();
        let all = HoldSet::of(Hold::Passive)
            .union(HoldSet::of(Hold::Held))
            .union(HoldSet::of(Hold::Running))
            .union(HoldSet::of(Hold::Player))
            .union(HoldSet::of(Hold::Streak))
            .union(HoldSet::of(Hold::Item))
            .union(HoldSet::of(Hold::Upgrade));
        assert_eq!(decode(0b111_1111), Some(all));
        assert_eq!(decode(0b1000_0000), None);
    }
}
