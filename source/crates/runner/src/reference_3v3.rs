use std::num::NonZeroU32;
use std::path::Path;

use campfire_capabilities::{InputValue, ModeInput};
use campfire_math::Tick;
use campfire_package::ModePackages;
use campfire_protocol::ServerSeed;

use crate::fixed_session::FixedSession;
use crate::runner::Runner;

/// The reference 3v3 as its packages hold it, at its slowest rate, which plays a match in the
/// fewest ticks, with six players of fixed keys.
#[derive(Debug)]
pub struct Reference3v3 {
    session: FixedSession,
}

const MODE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/moba/modes/3v3");
const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();
/// The hero each slot picks.
const HEROES: [&str; 6] = [
    "hero-cinder",
    "hero-gale",
    "hero-husk",
    "hero-kensho",
    "hero-rime",
    "hero-veil",
];

impl Reference3v3 {
    pub const PLAYERS: u32 = 6;

    pub fn load() -> Reference3v3 {
        let packages =
            ModePackages::from_dir(Path::new(MODE)).unwrap_or_else(|error| panic!("{error}"));
        Reference3v3 {
            session: FixedSession::new(packages, TICK_HZ, Reference3v3::PLAYERS),
        }
    }

    pub const fn packages(&self) -> &ModePackages {
        self.session.packages()
    }

    /// The seed of the log's first segment.
    pub fn seed() -> ServerSeed {
        FixedSession::seed()
    }

    /// A match at tick 0, in which each player picked a hero, in slot order, and two spells.
    pub fn start(&self) -> Runner {
        let mut fixed = self.session.start();
        for (slot, hero) in (0..).zip(HEROES) {
            let payload = ModeInput::payload(&[
                ModeInput {
                    name: "hero",
                    value: InputValue::String(hero),
                },
                ModeInput {
                    name: "spells",
                    value: InputValue::StringList(vec!["haste", "mend"]),
                },
            ]);
            fixed.send(slot, Tick::new(0), &payload);
        }
        fixed.into_runner()
    }
}
