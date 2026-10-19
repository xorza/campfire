use super::*;

#[test]
fn the_rules_allow_each_change_the_modes_players_allow() {
    let rules = |late_join, bot_takeover, leaver| {
        SeatRules::new(PlayersData {
            late_join,
            bot_takeover,
            leaver,
        })
    };
    let joined = |from| SeatChangeKind::Joined { from };
    let left = |becomes| SeatChangeKind::Left { becomes };
    let closed = rules(false, false, AfterLeave::Reserve);
    assert_eq!(closed.check(joined(Taken::Own)), Ok(()));
    assert_eq!(
        closed.check(joined(Taken::Open)),
        Err(SeatRuleError::LateJoin)
    );
    assert_eq!(
        closed.check(joined(Taken::Bot)),
        Err(SeatRuleError::BotTakeover)
    );
    assert_eq!(closed.check(left(AfterLeave::Reserve)), Ok(()));
    let open = rules(true, true, AfterLeave::Bot);
    assert_eq!(open.check(joined(Taken::Open)), Ok(()));
    assert_eq!(open.check(joined(Taken::Bot)), Ok(()));
    assert_eq!(open.check(left(AfterLeave::Bot)), Ok(()));
    for becomes in [AfterLeave::Reserve, AfterLeave::Open] {
        assert_eq!(
            open.check(left(becomes)),
            Err(SeatRuleError::Leaver { becomes })
        );
    }
    // Late join alone takes open seats, no bot's.
    let late = rules(true, false, AfterLeave::Open);
    assert_eq!(late.check(joined(Taken::Open)), Ok(()));
    assert_eq!(
        late.check(joined(Taken::Bot)),
        Err(SeatRuleError::BotTakeover)
    );
    assert_eq!(late.check(left(AfterLeave::Open)), Ok(()));
    // What a leaver's seat becomes, by each `leaver`.
    let becomes = [AfterLeave::Reserve, AfterLeave::Bot, AfterLeave::Open]
        .map(|leaver| rules(true, true, leaver).after_leave());
    assert_eq!(
        becomes,
        [AfterLeave::Reserve, AfterLeave::Bot, AfterLeave::Open]
    );
}
