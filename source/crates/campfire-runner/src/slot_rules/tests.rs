use super::*;

#[test]
fn the_rules_allow_each_change_the_modes_players_allow() {
    let rules = |late_join, bot_takeover, leaver| {
        SlotRules::new(PlayersData {
            late_join,
            bot_takeover,
            leaver,
        })
    };
    let joined = |from| SlotChangeKind::Joined { from };
    let left = |becomes| SlotChangeKind::Left { becomes };
    let closed = rules(false, false, Leaver::Reserve);
    assert_eq!(closed.check(joined(Taken::Own)), Ok(()));
    assert_eq!(
        closed.check(joined(Taken::Open)),
        Err(SlotRuleError::LateJoin)
    );
    assert_eq!(
        closed.check(joined(Taken::Bot)),
        Err(SlotRuleError::BotTakeover)
    );
    assert_eq!(closed.check(left(AfterLeave::Reserve)), Ok(()));
    let open = rules(true, true, Leaver::Bot);
    assert_eq!(open.check(joined(Taken::Open)), Ok(()));
    assert_eq!(open.check(joined(Taken::Bot)), Ok(()));
    assert_eq!(open.check(left(AfterLeave::Bot)), Ok(()));
    for becomes in [AfterLeave::Reserve, AfterLeave::Open] {
        assert_eq!(
            open.check(left(becomes)),
            Err(SlotRuleError::Leaver { becomes })
        );
    }
    // Late join alone takes open slots, no bot's.
    let late = rules(true, false, Leaver::Open);
    assert_eq!(late.check(joined(Taken::Open)), Ok(()));
    assert_eq!(
        late.check(joined(Taken::Bot)),
        Err(SlotRuleError::BotTakeover)
    );
    assert_eq!(late.check(left(AfterLeave::Open)), Ok(()));
    // What a leaver's slot becomes, by each `leaver`.
    let becomes = [Leaver::Reserve, Leaver::Bot, Leaver::Open]
        .map(|leaver| rules(true, true, leaver).after_leave());
    assert_eq!(
        becomes,
        [AfterLeave::Reserve, AfterLeave::Bot, AfterLeave::Open]
    );
}
