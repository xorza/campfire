use std::fmt::{self, Display, Formatter};

use clap::ValueEnum;

/// A play of the check, by the name its command line gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Play {
    /// Player 1 trains, builds and fights the skirmish's bot.
    Skirmish,
    /// China's free truck delivers supply.
    Supply,
    /// America's Chinook delivers supply, and its Comanche falls to GLA's base.
    Helicopter,
    /// America's Raptor takes off, strikes, lands and reloads.
    Jet,
    /// America learns the Paladin's science, and its war factory then trains one.
    Science,
    /// A Gattling Cannon kills a dozer.
    Gattling,
    /// A Patriot battery kills a dozer.
    Patriot,
    /// A Stinger site kills a dozer.
    Stinger,
    /// A Tank Hunter's missiles kill a dozer.
    Missile,
    /// America's Comanche chooses its weapons, its Crusaders answer a rocket buggy, and one fires
    /// from its turret as it drives.
    Combat,
    /// America's Chinook flies a Ranger and a Crusader across the map, its Ranger and Missile
    /// Defender fire from a Humvee, and its Crusaders destroy China's Troop Crawler of Red Guards.
    Transport,
    /// China's Red Guards leave their Troop Crawlers and walk into one, a Helix and an Overlord's
    /// bunker.
    Entry,
    /// GLA's Rebels garrison a free building, outrange America's Ranger, and leave it as
    /// America's Crusaders shell it.
    Garrison,
    /// Two attackers from base starts.
    Base,
    /// China's attacker against GLA's builder from standard starts.
    Attack,
    /// A dozer drives from player 1's start to player 2's.
    Drive,
}

/// Its name on the command line, which names its probe's file.
impl Display for Play {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let value = self.to_possible_value().expect("the check skips no play");
        f.write_str(value.get_name())
    }
}
