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

impl Play {
    /// Its name on the command line, which names its probe's file.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Skirmish => "skirmish",
            Self::Supply => "supply",
            Self::Helicopter => "helicopter",
            Self::Jet => "jet",
            Self::Science => "science",
            Self::Gattling => "gattling",
            Self::Patriot => "patriot",
            Self::Stinger => "stinger",
            Self::Missile => "missile",
            Self::Combat => "combat",
            Self::Transport => "transport",
            Self::Garrison => "garrison",
            Self::Base => "base",
            Self::Attack => "attack",
            Self::Drive => "drive",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plays_name_is_the_one_its_command_line_reads() {
        for &play in Play::value_variants() {
            let value = play.to_possible_value().unwrap();
            assert_eq!(value.get_name(), play.name());
            assert_eq!(Play::from_str(play.name(), false), Ok(play));
        }
    }
}
