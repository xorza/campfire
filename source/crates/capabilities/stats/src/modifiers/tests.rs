use std::num::NonZeroU32;

use campfire_codec::Binary;
use campfire_common::Ticks;
use campfire_sim::IdAllocator;
use campfire_values::NameId;

use super::*;
use crate::interval::Interval;
use crate::lifetime::Lifetime;
/// An application of modifier `id` from `source`, reapplied as `reapply`, up to
/// `max_stacks`, adding `armor` a stack, holding until tick `until`, and with `stack` =
/// `(now, life)`, its stack applied in tick `now` for `life` ticks.
fn applied(
    id: u16,
    source: Option<StableId>,
    reapply: Reapply,
    max_stacks: Option<u32>,
    armor: i64,
    until: Option<u64>,
    stack: Option<(u64, u64)>,
) -> Application {
    let stack_life = stack.map(|(_, life)| Ticks::new(life));
    let stack_ends = stack.map(|(now, life)| StackEnd {
        until: Instance::end(Tick::new(now), Ticks::new(life)),
        count: 1,
    });
    Application {
        instance: NewInstance {
            lifetime: Lifetime::applied(
                until.map_or(Ends::Never, |until| Ends::At(Tick::new(until))),
            ),
            stack_life,
            stack_end: stack_ends,
            stats: vec![StatShare {
                value: Num::int(armor),
                live: None,
            }],
            state: vec![StateValue::Int(7)],
            ..NewInstance::bare(ModifierId::new(id), source)
        },
        reapply,
        max_stacks: max_stacks.and_then(NonZeroU32::new),
    }
}

/// Lets `hold` keep the instance of `id` from `source` too, as a held application that
/// changes nothing else, `ignore`, joins it.
fn hold(
    (modifiers, clocks): (&mut Modifiers, &mut ModifierClocks),
    id: ModifierId,
    source: Option<StableId>,
    hold: Hold,
) {
    let mut held = Application {
        instance: NewInstance::bare(id, source),
        reapply: Reapply::Ignore,
        max_stacks: None,
    };
    held.instance.lifetime = Lifetime::held(hold);
    assert!(
        modifiers.get(id, source).is_some(),
        "an instance it carries"
    );
    modifiers.apply(clocks, held);
}

fn stacks(modifiers: &Modifiers) -> Vec<(u16, u32)> {
    modifiers
        .iter()
        .map(|instance| (u16::try_from(instance.id.index()).unwrap(), instance.stacks))
        .collect()
}

fn ends(carried: Carried<'_>) -> Vec<(u64, u32)> {
    let ends = carried.stack_ends.iter();
    ends.map(|end| (end.until.get(), end.count)).collect()
}

#[test]
fn a_hold_and_a_timed_application_of_one_instance_keep_both_lifetimes() {
    let mut ids = IdAllocator::default();
    let a = Some(ids.allocate());
    let id = ModifierId::new(0);
    let held = || {
        let mut held = applied(0, a, Reapply::Refresh, None, 1, None, None);
        held.instance.lifetime = Lifetime::held(Hold::Held);
        held
    };
    let timed = || applied(0, a, Reapply::Refresh, None, 1, Some(6), None);
    // Held by an aura, then applied until tick 6: the aura lets go in tick 2, and the
    // application keeps it through tick 5.
    let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
    modifiers.apply(&mut clocks, held());
    modifiers.apply(&mut clocks, timed());
    assert!(modifiers.release_held(&mut clocks, |_, _| false));
    assert!(modifiers.get(id, a).is_some());
    modifiers.expire(&mut clocks, Tick::new(5));
    assert!(modifiers.get(id, a).is_some());
    modifiers.expire(&mut clocks, Tick::new(6));
    assert!(modifiers.get(id, a).is_none());
    // Applied until tick 6, then held: past tick 6 the hold keeps it, with no end; when the
    // aura lets go, it ends.
    let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
    modifiers.apply(&mut clocks, timed());
    hold((&mut modifiers, &mut clocks), id, a, Hold::Held);
    modifiers.expire(&mut clocks, Tick::new(8));
    let kept = modifiers.get(id, a).unwrap();
    assert_eq!(kept.lifetime.until(), None);
    assert!(modifiers.release_held(&mut clocks, |_, _| false));
    assert!(modifiers.get(id, a).is_none());
    // Of a modifier that ignores a second application: applied until tick 6, then held by an
    // aura's application, whose hold joins it all the same, and its numbers stay: past tick 6
    // the hold keeps it, with its 1 armor.
    let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
    modifiers.apply(
        &mut clocks,
        applied(0, a, Reapply::Ignore, None, 1, Some(6), None),
    );
    let mut aura = applied(0, a, Reapply::Ignore, None, 5, None, None);
    aura.instance.lifetime = Lifetime::held(Hold::Held);
    let touched = modifiers.apply(&mut clocks, aura.clone());
    assert_eq!(
        touched,
        Touched {
            stats: true,
            clocks: false
        }
    );
    assert_eq!(modifiers.apply(&mut clocks, aura), Touched::default());
    modifiers.expire(&mut clocks, Tick::new(8));
    let kept = modifiers.get(id, a).unwrap();
    assert!(kept.lifetime.held_by(Hold::Held));
    assert_eq!(kept.shares[0].value, Num::ONE);
    // A passive whose rank no longer keeps it lets go of its hold alone: an application of
    // its own, with no end, keeps it.
    let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
    modifiers.apply(
        &mut clocks,
        applied(0, a, Reapply::Refresh, None, 1, None, None),
    );
    hold((&mut modifiers, &mut clocks), id, a, Hold::Passive);
    modifiers.release(&mut clocks, id, a, Hold::Passive);
    assert!(modifiers.get(id, a).is_some());
}

#[test]
fn an_interval_refreshed_keeps_its_next_tick() {
    let a = Some(IdAllocator::default().allocate());
    // An interval of 3 ticks, next in tick 5, refreshed to 2 ticks, from tick 9: the next
    // stays in tick 5, and the one after comes 2 ticks later, in tick 7.
    let interval = |every, next| {
        let mut application = applied(3, a, Reapply::Refresh, None, 0, None, None);
        application.instance.interval = Some(Interval {
            every: Ticks::new(every),
            next: Tick::new(next),
        });
        application
    };
    let (mut timed, mut timed_clocks) = (Modifiers::default(), ModifierClocks::default());
    timed.apply(&mut timed_clocks, interval(3, 5));
    timed.apply(&mut timed_clocks, interval(2, 9));
    let due = [4, 5, 6, 7]
        .map(|now| timed_clocks.advance_intervals(&timed, Tick::new(now), |_| true, |_, _| {}));
    assert_eq!(due, [false, true, false, true]);
}

#[test]
fn modifiers_refresh_stack_to_their_limit_ignore_and_end() {
    let mut ids = IdAllocator::default();
    let (a, b) = (Some(ids.allocate()), Some(ids.allocate()));
    let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
    // Refreshing takes the new numbers and end, and keeps the stacks and state: armor 5 to
    // 10, the end 10 to 20.
    modifiers.apply(
        &mut clocks,
        applied(0, a, Reapply::Refresh, None, 5, Some(10), None),
    );
    let at = modifiers.position(ModifierId::new(0), a).unwrap();
    clocks.set_state(at, &[StateValue::Int(9)]);
    modifiers.apply(
        &mut clocks,
        applied(0, a, Reapply::Refresh, None, 10, Some(20), None),
    );
    let held = modifiers.get(ModifierId::new(0), a).unwrap();
    assert_eq!(
        (held.stacks, held.lifetime.until()),
        (1, Some(Tick::new(20)))
    );
    assert_eq!(
        (held.shares[0].value, &clocks.state(at)[0]),
        (Num::int(10), &StateValue::Int(9))
    );
    // From another source, another instance, kept after the first by source.
    modifiers.apply(
        &mut clocks,
        applied(0, b, Reapply::Refresh, None, 1, None, None),
    );
    // Stacking to a limit of 3, each stack for 3 ticks, applied in ticks 0 to 3: 1, 2, 3,
    // then 3 stacks; they end as ticks 4, 5 and 6 start, and the fourth, ending as 7
    // starts, takes the place of the one that ends soonest.
    for (now, stacks) in [(0, 1), (1, 2), (2, 3), (3, 3)] {
        let stack = Some((now, 3));
        modifiers.apply(
            &mut clocks,
            applied(1, a, Reapply::Stack, Some(3), 2, None, stack),
        );
        assert_eq!(modifiers.get(ModifierId::new(1), a).unwrap().stacks, stacks);
    }
    let held = modifiers.get(ModifierId::new(1), a).unwrap();
    assert_eq!(ends(held), [(5, 1), (6, 1), (7, 1)]);
    // Ignoring leaves it as it was.
    modifiers.apply(
        &mut clocks,
        applied(2, None, Reapply::Ignore, None, 3, Some(8), None),
    );
    modifiers.apply(
        &mut clocks,
        applied(2, None, Reapply::Ignore, None, 30, Some(80), None),
    );
    let ignored = modifiers.get(ModifierId::new(2), None).unwrap();
    assert_eq!(
        (ignored.shares[0].value, ignored.lifetime.until()),
        (Num::int(3), Some(Tick::new(8)))
    );
    assert_eq!(stacks(&modifiers), [(0, 1), (0, 1), (1, 3), (2, 1)]);

    // As tick 5 starts nothing has ended but the first stack; as 6 starts, the second; as 7
    // starts the third, and as 8 the last, which ends its instance, and the ignored one.
    assert!(modifiers.expire(&mut clocks, Tick::new(5)).stats);
    assert_eq!(stacks(&modifiers), [(0, 1), (0, 1), (1, 2), (2, 1)]);
    assert!(!modifiers.expire(&mut clocks, Tick::new(5)).stats);
    assert!(modifiers.expire(&mut clocks, Tick::new(8)).stats);
    assert_eq!(stacks(&modifiers), [(0, 1), (0, 1)]);

    // A stack of a shorter life, as at another rank, ends in its order: applied in tick 10
    // for 20 ticks, it ends as 31 starts; in tick 11 for 2, as 14 starts.
    modifiers.apply(
        &mut clocks,
        applied(1, a, Reapply::Stack, None, 2, None, Some((10, 20))),
    );
    modifiers.apply(
        &mut clocks,
        applied(1, a, Reapply::Stack, None, 2, None, Some((11, 2))),
    );
    let id = ModifierId::new(1);
    assert_eq!(ends(modifiers.get(id, a).unwrap()), [(14, 1), (31, 1)]);
    // Writing 4 stacks in tick 12 adds two that end as stacks of 2 ticks applied then do,
    // as 15 starts; writing 3 takes away the one that ends soonest.
    modifiers.set_stacks(id, a, 4, Tick::new(12));
    assert_eq!(
        ends(modifiers.get(id, a).unwrap()),
        [(14, 1), (15, 2), (31, 1)]
    );
    modifiers.set_stacks(id, a, 3, Tick::new(12));
    assert_eq!(ends(modifiers.get(id, a).unwrap()), [(15, 2), (31, 1)]);
    // As 15 starts two of the 3 end; writing 0 then keeps the instance, with no ends.
    assert!(modifiers.expire(&mut clocks, Tick::new(15)).stats);
    assert_eq!(
        (
            modifiers.get(id, a).unwrap().stacks,
            ends(modifiers.get(id, a).unwrap())
        ),
        (1, vec![(31, 1)])
    );
    modifiers.set_stacks(id, a, 0, Tick::new(16));
    assert_eq!(
        (
            modifiers.get(id, a).unwrap().stacks,
            ends(modifiers.get(id, a).unwrap())
        ),
        (0, vec![])
    );

    // A passive whose last stack ends stays, with none: one stack of 2 ticks applied in
    // tick 16 ends as 19 starts.
    modifiers.apply(
        &mut clocks,
        applied(3, a, Reapply::Stack, None, 1, None, Some((16, 2))),
    );
    hold(
        (&mut modifiers, &mut clocks),
        ModifierId::new(3),
        a,
        Hold::Passive,
    );
    assert!(modifiers.expire(&mut clocks, Tick::new(19)).stats);
    assert_eq!(modifiers.get(ModifierId::new(3), a).unwrap().stacks, 0);

    // Removing one, then a death, which keeps only passives.
    assert!(modifiers.remove(&mut clocks, ModifierId::new(0), b));
    assert!(!modifiers.remove(&mut clocks, ModifierId::new(0), b));
    hold(
        (&mut modifiers, &mut clocks),
        ModifierId::new(0),
        a,
        Hold::Passive,
    );
    modifiers.clear_on_death(&mut clocks);
    assert_eq!(stacks(&modifiers), [(0, 1), (3, 0)]);
}

#[test]
fn modifiers_decode_only_with_ordered_ends_that_count_their_stacks() {
    let decode = |modifiers: &Modifiers| {
        let bytes = Binary::encode(modifiers);
        Binary::decode::<Modifiers>(&bytes).is_ok()
    };
    // Stacks of 4 ticks applied in ticks 0, 4 and 4, each ending as the tick after its last
    // starts: ends in tick 5, of one stack, and in tick 9, of two, for its 3 stacks.
    let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
    for now in [0, 4, 4] {
        let stack = applied(0, None, Reapply::Stack, None, 1, None, Some((now, 4)));
        modifiers.apply(&mut clocks, stack);
    }
    let ends = |modifiers: &Modifiers| ends(modifiers.get(ModifierId::new(0), None).unwrap());
    assert_eq!(ends(&modifiers), [(5, 1), (9, 2)]);
    assert!(decode(&modifiers));
    // Uncounted, unordered, with an empty end, and lifeless.
    let flaws: [fn(&mut Modifiers); 4] = [
        |bad| bad.instances[0].stacks = 4,
        |bad| bad.ends.swap(0, 1),
        |bad| {
            bad.ends[0].count = 0;
            bad.instances[0].stacks = 2;
        },
        |bad| bad.instances[0].stack_life = None,
    ];
    for flaw in flaws {
        let mut bad = modifiers.clone();
        flaw(&mut bad);
        assert!(!decode(&bad), "{bad:?}");
    }
}
