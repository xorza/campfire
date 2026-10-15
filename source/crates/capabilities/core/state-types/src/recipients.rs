use std::fmt;
use std::ops::BitOr;

/// The clients beside an entity's owner that receive a kind of its state (design 14, D1): its
/// team's other players, the other teams' players, and the links with no slot, each among those
/// that hold the entity. The owner receives every kind but the server's, so no set names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[must_use]
pub struct Recipients(u8);

impl Recipients {
    /// The owner alone.
    pub const NONE: Recipients = Recipients(0);
    pub const ALLIES: Recipients = Recipients(1);
    pub const ENEMIES: Recipients = Recipients(1 << 1);
    pub const SPECTATORS: Recipients = Recipients(1 << 2);
    pub const ALL: Recipients = Recipients(0b111);

    pub const fn union(self, other: Recipients) -> Recipients {
        Recipients(self.0 | other.0)
    }

    /// Its recipients but those of `other`.
    pub const fn without(self, other: Recipients) -> Recipients {
        Recipients(self.0 & !other.0)
    }

    /// Whether it holds every recipient of `other`.
    pub const fn contains(self, other: Recipients) -> bool {
        self.0 & other.0 == other.0
    }

    /// The byte the session's terms hold it as: allies 1, enemies 2, spectators 4.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The set of the byte `bits`; `None` for a byte with another bit.
    pub const fn from_bits(bits: u8) -> Option<Recipients> {
        if bits & !Recipients::ALL.0 == 0 {
            Some(Recipients(bits))
        } else {
            None
        }
    }

    /// The recipient of the name `name` in a mode's `[visibility]` or a host's settings.
    pub fn named(name: &str) -> Option<Recipients> {
        Recipients::NAMED
            .into_iter()
            .find_map(|(named, recipient)| (named == name).then_some(recipient))
    }

    /// The names of its recipients, in the order allies, enemies, spectators.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        Recipients::NAMED
            .into_iter()
            .filter_map(move |(name, recipient)| self.contains(recipient).then_some(name))
    }

    const NAMED: [(&'static str, Recipients); 3] = [
        ("allies", Recipients::ALLIES),
        ("enemies", Recipients::ENEMIES),
        ("spectators", Recipients::SPECTATORS),
    ];
}

/// Its recipients' names, as a mode's `[visibility]` writes them, joined by commas: `none` for
/// the owner alone.
impl fmt::Display for Recipients {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut names = self.names();
        let Some(first) = names.next() else {
            return f.write_str("none");
        };
        f.write_str(first)?;
        for name in names {
            write!(f, ", {name}")?;
        }
        Ok(())
    }
}

impl BitOr for Recipients {
    type Output = Recipients;

    fn bitor(self, other: Recipients) -> Recipients {
        self.union(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_holds_what_its_parts_hold() {
        let ally_spectator = Recipients::ALLIES | Recipients::SPECTATORS;
        assert!(ally_spectator.contains(Recipients::ALLIES));
        assert!(ally_spectator.contains(Recipients::SPECTATORS));
        assert!(!ally_spectator.contains(Recipients::ENEMIES));
        assert!(!ally_spectator.contains(Recipients::ALL));
        assert!(Recipients::ALL.contains(ally_spectator));
        // Every set holds the owner alone.
        assert!(Recipients::NONE.contains(Recipients::NONE));
        assert!(ally_spectator.contains(Recipients::NONE));
        assert_eq!(
            Recipients::ALLIES | Recipients::ENEMIES | Recipients::SPECTATORS,
            Recipients::ALL
        );
        assert_eq!(Recipients::ALL.without(ally_spectator), Recipients::ENEMIES);
        // Its text: its names, by commas, or none.
        assert_eq!(ally_spectator.to_string(), "allies, spectators");
        assert_eq!(Recipients::ENEMIES.to_string(), "enemies");
        assert_eq!(Recipients::NONE.to_string(), "none");
    }

    #[test]
    fn a_set_reads_back_from_its_byte_and_its_names() {
        // Allies 1, enemies 2, spectators 4; a byte with any other bit is no set.
        for bits in 0..=7 {
            let set = Recipients::from_bits(bits).unwrap();
            assert_eq!(set.bits(), bits);
            let names: Vec<&str> = set.names().collect();
            let read = names
                .iter()
                .map(|name| Recipients::named(name).unwrap())
                .fold(Recipients::NONE, Recipients::union);
            assert_eq!(read, set, "{names:?}");
        }
        assert_eq!(Recipients::from_bits(8), None);
        assert_eq!(Recipients::from_bits(0xff), None);
        let names: Vec<&str> = (Recipients::SPECTATORS | Recipients::ALLIES)
            .names()
            .collect();
        assert_eq!(names, ["allies", "spectators"]);
        assert_eq!(Recipients::named("owner"), None);
        assert_eq!(Recipients::named("Allies"), None);
    }
}
