use super::*;

#[test]
fn the_rules_allow_each_change_the_modes_players_allow() {
    let rules = |late_join, bot_takeover, leaver| {
        PlayerSlotRules::new(PlayersData {
            late_join,
            bot_takeover,
            leaver,
            ..PlayersData::DEFAULT
        })
    };
    let joined = |from| PlayerSlotChangeKind::Joined { from };
    let left = |becomes| PlayerSlotChangeKind::Left { becomes };
    let closed = rules(false, false, AfterLeave::Reserve);
    assert_eq!(closed.check(joined(Taken::Own)), Ok(()));
    assert_eq!(
        closed.check(joined(Taken::Open)),
        Err(PlayerSlotRuleError::LateJoin)
    );
    assert_eq!(
        closed.check(joined(Taken::Bot)),
        Err(PlayerSlotRuleError::BotTakeover)
    );
    assert_eq!(closed.check(left(AfterLeave::Reserve)), Ok(()));
    let open = rules(true, true, AfterLeave::Bot);
    assert_eq!(open.check(joined(Taken::Open)), Ok(()));
    assert_eq!(open.check(joined(Taken::Bot)), Ok(()));
    assert_eq!(open.check(left(AfterLeave::Bot)), Ok(()));
    for becomes in [AfterLeave::Reserve, AfterLeave::Open] {
        assert_eq!(
            open.check(left(becomes)),
            Err(PlayerSlotRuleError::Leaver { becomes })
        );
    }
    // Late join alone takes open player slots, no bot's.
    let late = rules(true, false, AfterLeave::Open);
    assert_eq!(late.check(joined(Taken::Open)), Ok(()));
    assert_eq!(
        late.check(joined(Taken::Bot)),
        Err(PlayerSlotRuleError::BotTakeover)
    );
    assert_eq!(late.check(left(AfterLeave::Open)), Ok(()));
    // What a leaver's player slot becomes, by each `leaver`.
    let becomes = [AfterLeave::Reserve, AfterLeave::Bot, AfterLeave::Open]
        .map(|leaver| rules(true, true, leaver).after_leave());
    assert_eq!(
        becomes,
        [AfterLeave::Reserve, AfterLeave::Bot, AfterLeave::Open]
    );
}
