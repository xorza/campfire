use super::*;

#[test]
fn a_slot_fired_in_the_tick_its_ready_tick_rose() {
    let tick = Tick::new;
    let mut shots = SlotShots::default();
    // The first ticks fire none; slot 1's rise at tick 10 fires it then, and slot 0's fall none.
    shots.see([tick(5), tick(8)].into_iter(), tick(9));
    assert_eq!([shots.fired(0), shots.fired(1)], [None, None]);
    shots.see([tick(4), tick(20)].into_iter(), tick(10));
    assert_eq!([shots.fired(0), shots.fired(1)], [None, Some(tick(10))]);
    // Ticks of other slots start again, of no shot; a slot past them fired none.
    shots.see([tick(30)].into_iter(), tick(11));
    assert_eq!([shots.fired(0), shots.fired(1)], [None, None]);
}

#[test]
fn a_slots_phase_is_its_shot_its_pre_attack_its_reload_or_its_wait() {
    let tick = Tick::new;
    let attack = SlotAttack {
        attacking: true,
        striking: false,
        pre_attack_until: None,
        ready_at: None,
        reloads: false,
        clip_shots: None,
        fired: 0,
        now: tick(10),
    };
    let phase = |change: fn(&mut SlotAttack)| {
        let mut slot = attack;
        change(&mut slot);
        slot.phase()
    };
    // Ready, of no pre-attack: none; striking: its shot, whatever else holds.
    assert_eq!(phase(|_| {}), None);
    assert_eq!(
        phase(|slot| {
            slot.striking = true;
            slot.pre_attack_until = Some(Tick::new(12));
            slot.ready_at = Some(Tick::new(12));
        }),
        Some(AttackPhase::Firing)
    );
    // A pre-attack until tick 11 holds at tick 10, and not one until tick 10.
    assert_eq!(
        phase(|slot| slot.pre_attack_until = Some(Tick::new(11))),
        Some(AttackPhase::PreAttack)
    );
    assert_eq!(
        phase(|slot| slot.pre_attack_until = Some(Tick::new(10))),
        None
    );
    // Ready at tick 11: it waits, or reloads a clip its shot emptied; ready at 10, none.
    assert_eq!(
        phase(|slot| slot.ready_at = Some(Tick::new(11))),
        Some(AttackPhase::Between)
    );
    assert_eq!(
        phase(|slot| {
            slot.ready_at = Some(Tick::new(11));
            slot.reloads = true;
        }),
        Some(AttackPhase::Reloading)
    );
    assert_eq!(phase(|slot| slot.ready_at = Some(Tick::new(10))), None);
    // A unit that does not attack holds no phase.
    assert_eq!(
        phase(|slot| {
            slot.attacking = false;
            slot.striking = true;
        }),
        None
    );
}

#[test]
fn a_clip_lacks_its_fired_shots_and_every_round_as_it_reloads_until_ready() {
    // A clip of 4 at tick 10: none fired lacks none; 3 fired, 3; emptied, full again and ready at
    // tick 12, all 4 as it reloads; ready at tick 10, none. A weapon of no clip lacks none.
    fn reloading(slot: &mut SlotAttack) {
        slot.reloads = true;
        slot.ready_at = Some(Tick::new(12));
    }
    fn one(slot: &mut SlotAttack) {
        slot.clip_shots = NonZeroU16::new(1);
    }
    let attack = SlotAttack {
        attacking: false,
        striking: false,
        pre_attack_until: None,
        ready_at: None,
        reloads: false,
        clip_shots: NonZeroU16::new(4),
        fired: 0,
        now: Tick::new(10),
    };
    let lack = |change: fn(&mut SlotAttack)| {
        let mut slot = attack;
        change(&mut slot);
        slot.lack()
    };
    let lacks = |rounds, reloads| ClipLack { rounds, reloads };
    assert_eq!(lack(|_| {}), lacks(0, false));
    assert_eq!(lack(|slot| slot.fired = 3), lacks(3, false));
    assert_eq!(lack(reloading), lacks(4, true));
    assert_eq!(
        lack(|slot| {
            slot.reloads = true;
            slot.ready_at = Some(Tick::new(10));
        }),
        lacks(0, false)
    );
    assert_eq!(
        lack(|slot| {
            reloading(slot);
            slot.clip_shots = None;
        }),
        lacks(0, false)
    );
    // A clip of one, which each shot empties, lacks its round as it reloads, until ready at tick
    // 12, and none once ready at tick 10.
    assert_eq!(
        lack(|slot| {
            reloading(slot);
            one(slot);
        }),
        lacks(1, true)
    );
    assert_eq!(
        lack(|slot| {
            one(slot);
            slot.reloads = true;
            slot.ready_at = Some(Tick::new(10));
        }),
        lacks(0, false)
    );
}
