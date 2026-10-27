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
    /// A Gattling Cannon kills a dozer.
    Gattling,
    /// A Patriot battery kills a dozer.
    Patriot,
    /// A Stinger site kills a dozer.
    Stinger,
    /// A Tank Hunter's missiles kill a dozer.
    Missile,
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
            Self::Gattling => "gattling",
            Self::Patriot => "patriot",
            Self::Stinger => "stinger",
            Self::Missile => "missile",
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
