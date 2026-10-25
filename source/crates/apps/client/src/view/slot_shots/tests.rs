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
