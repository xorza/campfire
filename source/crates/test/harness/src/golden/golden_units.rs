use std::fmt::{self, Write as _};

use campfire_capabilities::Match;
use campfire_common::PlayerSlot;
use campfire_players::PlayersWorld;
use campfire_sim::{EntityIndex, Position};
use campfire_stats::PoolMeters;
use campfire_units::{Dead, Owner, PoolId, UnitTypeBook};
use campfire_values::{NameId as _, ResourceId, Team, UnitType};

/// The units of a golden match as text in samples at a period, one line a unit by stable id, then
/// each player's resources: the parts of units and players the behaviour digest hashes, as a tick's
/// deaths show as units dead or gone, and its failed calls in the digest alone. A moved golden
/// names the first sample and the lines that differ, and a bless's diff shows them, as Factorio's
/// heavy mode writes its states as text a diff tool compares
/// (02-engine-core.md, A golden says what moved).
#[derive(Debug, Default)]
pub(crate) struct GoldenUnits {
    text: String,
}

/// The first sample whose lines differ from the pinned ones: its header, and for each line that
/// differs, a unit's or a player's, as pinned and as run, either none where the other has none.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Moved<'a> {
    pub(crate) header: &'a str,
    pub(crate) lines: Vec<MovedLine<'a>>,
}

/// A line of a sample that differs: as pinned and as run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MovedLine<'a> {
    pub(crate) pinned: Option<&'a str>,
    pub(crate) run: Option<&'a str>,
}

impl GoldenUnits {
    /// The ticks between two samples: 10 s at 30 ticks a second, so even the shortest golden
    /// match, of 600 ticks, writes two.
    pub(crate) const EVERY: u64 = 300;

    /// The text, as its file holds it.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Writes the sample of `game` after its tick `tick`, of `players` players and
    /// `resources`.
    pub(crate) fn record(
        &mut self,
        game: &Match,
        tick: u64,
        players: u32,
        resources: &[ResourceId],
    ) {
        debug_assert_eq!(tick % Self::EVERY, 0, "a sample's tick");
        let names = game.get_resource::<UnitTypeBook>().map(UnitTypeBook::types);
        writeln!(self.text, "tick {tick}").expect("text writes");
        let units = game.resource::<EntityIndex>().iter();
        for (id, entity) in units.filter(|(id, _)| id.is_allocated()) {
            let unit = game.entity(entity);
            let line = &mut self.text;
            write!(line, "#{}", id.get()).expect("text writes");
            if let Some(&unit_type) = unit.get::<UnitType>() {
                match names.and_then(|names| names.name(unit_type)) {
                    Some(name) => write!(line, " {name}"),
                    None => write!(line, " {unit_type:?}"),
                }
                .expect("text writes");
            }
            if let Some(team) = unit.get::<Team>() {
                write!(line, " team {}", team.get()).expect("text writes");
            }
            if let Some(owner) = unit.get::<Owner>() {
                write!(line, " slot {}", owner.player_slot().get()).expect("text writes");
            }
            if let Some(position) = unit.get::<Position>() {
                let at = position.get();
                write!(line, " at {:?}, {:?}, {:?}", at.x, at.y, at.z).expect("text writes");
            }
            let meters = PoolMeters::of(game, entity);
            for pool in (0..).map_while(PoolId::new) {
                if let Some(current) = meters.current(pool) {
                    write!(line, " pool{}={current:?}", pool.index()).expect("text writes");
                }
            }
            if unit.contains::<Dead>() {
                line.push_str(" dead");
            }
            line.push('\n');
        }
        for slot in (0..players).map(PlayerSlot::new) {
            let Some(amounts) = game.player_resources(slot) else {
                continue;
            };
            write!(self.text, "slot {}:", slot.get()).expect("text writes");
            for &resource in resources {
                write!(self.text, " {}", amounts.amount(resource)).expect("text writes");
            }
            self.text.push('\n');
        }
        self.text.push('\n');
    }

    /// The first sample of `run` whose lines differ from `pinned`'s, with each line that
    /// differs, by the unit or the player it names; none when every sample of both is one.
    pub(crate) fn moved<'a>(pinned: &'a str, run: &'a str) -> Option<Moved<'a>> {
        let blocks = |text: &'a str| text.split_terminator("\n\n");
        let mut pinned_blocks = blocks(pinned);
        let mut run_blocks = blocks(run);
        loop {
            let (was, is) = (pinned_blocks.next(), run_blocks.next());
            if was.is_none() && is.is_none() {
                return None;
            }
            if was == is {
                continue;
            }
            // A block of no header, which no sample writes, still differs.
            let header = is.or(was).and_then(|block| block.lines().next());
            let header = header.unwrap_or("a sample of no header");
            return Some(Moved {
                header,
                lines: Self::differing(was.unwrap_or_default(), is.unwrap_or_default()),
            });
        }
    }

    /// The lines of two blocks that differ, matched by what they name, a unit's `#id` or a
    /// player's `slot n:`, in the order of the pinned block, then those only the run holds.
    fn differing<'a>(was: &'a str, is: &'a str) -> Vec<MovedLine<'a>> {
        let key = |line: &'a str| match line.split_once(':') {
            Some((slot, _)) if line.starts_with("slot ") => slot,
            _ => line.split(' ').next().unwrap_or_default(),
        };
        let was: Vec<&str> = was.lines().skip(1).collect();
        let is: Vec<&str> = is.lines().skip(1).collect();
        let mut moved: Vec<MovedLine<'a>> = was
            .iter()
            .filter_map(|&pinned| {
                let run = is.iter().copied().find(|&line| key(line) == key(pinned));
                (run != Some(pinned)).then_some(MovedLine {
                    pinned: Some(pinned),
                    run,
                })
            })
            .collect();
        moved.extend(
            is.iter()
                .filter(|&&line| !was.iter().any(|&pinned| key(pinned) == key(line)))
                .map(|&run| MovedLine {
                    pinned: None,
                    run: Some(run),
                }),
        );
        moved
    }
}

/// The sample and the lines that differ, the first `SHOWN` of them, each as pinned and as run.
impl fmt::Display for Moved<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const SHOWN: usize = 8;
        let lines = self.lines.len();
        let noun = if lines == 1 {
            "line differs"
        } else {
            "lines differ"
        };
        write!(f, "at {}, {lines} {noun}:", self.header)?;
        let text = |line: Option<&str>| line.unwrap_or("(none)").to_owned();
        for line in self.lines.iter().take(SHOWN) {
            let (pinned, run) = (text(line.pinned), text(line.run));
            write!(f, "\n  pinned {pinned}\n  run    {run}")?;
        }
        if lines > SHOWN {
            write!(f, "\n  and {} more", lines - SHOWN)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sample_that_differs_names_each_line_that_moved() {
        let pinned = "tick 0\n#1 walker at 0, 0, 0\nslot 0: 5\n\n\
                      tick 300\n#1 walker at 0, 0, 3\n#2 creep at 1, 0, 0\n#3 tower\nslot 0: 5\n\n";
        assert_eq!(GoldenUnits::moved(pinned, pinned), None);

        // At tick 300 the walker stands elsewhere, the creep is gone, the tower is as it was, a
        // new unit 4 stands, and the slot holds more; tick 0 is the same, so it is not named.
        let run = "tick 0\n#1 walker at 0, 0, 0\nslot 0: 5\n\n\
                   tick 300\n#1 walker at 0, 0, 2.5\n#3 tower\n#4 creep at 9, 0, 0\nslot 0: 7\n\n";
        let moved = GoldenUnits::moved(pinned, run).unwrap();
        let line = |pinned, run| MovedLine { pinned, run };
        assert_eq!(
            moved,
            Moved {
                header: "tick 300",
                lines: vec![
                    line(Some("#1 walker at 0, 0, 3"), Some("#1 walker at 0, 0, 2.5")),
                    line(Some("#2 creep at 1, 0, 0"), None),
                    line(Some("slot 0: 5"), Some("slot 0: 7")),
                    line(None, Some("#4 creep at 9, 0, 0")),
                ],
            }
        );
        assert_eq!(
            moved.to_string(),
            "at tick 300, 4 lines differ:\
             \n  pinned #1 walker at 0, 0, 3\n  run    #1 walker at 0, 0, 2.5\
             \n  pinned #2 creep at 1, 0, 0\n  run    (none)\
             \n  pinned slot 0: 5\n  run    slot 0: 7\
             \n  pinned (none)\n  run    #4 creep at 9, 0, 0"
        );

        // Two slots are told apart, as each line names its own.
        let slots = |amounts: [u8; 2]| {
            format!("tick 0\nslot 0: {}\nslot 1: {}\n\n", amounts[0], amounts[1])
        };
        let (was, is) = (slots([5, 5]), slots([5, 6]));
        let moved = GoldenUnits::moved(&was, &is).unwrap();
        assert_eq!(moved.lines, [line(Some("slot 1: 5"), Some("slot 1: 6"))]);
        assert!(moved.to_string().starts_with("at tick 0, 1 line differs:"));

        // A block of no header, which no sample writes, still differs.
        let headless = GoldenUnits::moved("tick 0\n#1 walker\n\n", "\n\n").unwrap();
        assert_eq!(headless.header, "a sample of no header");

        // A run that ends before a pinned sample names that sample, every line of it pinned alone.
        let short = "tick 0\n#1 walker at 0, 0, 0\nslot 0: 5\n\n";
        let moved = GoldenUnits::moved(pinned, short).unwrap();
        assert_eq!(moved.header, "tick 300");
        assert!(moved.lines.iter().all(|line| line.run.is_none()));
        assert_eq!(moved.lines.len(), 4);
    }
}
