use campfire_log::internals::round_trip;
use campfire_values::Team;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    for result in [MatchResult::Won(Team::new(1)), MatchResult::Draw] {
        let end = MatchEnd::new(Tick::new(900), result);
        assert_eq!(
            MatchEnded::of(end),
            MatchEnded {
                tick: Tick::new(900),
                result,
            }
        );
        round_trip(&MatchEnded::of(end));
    }
}
