use super::*;

#[test]
fn a_mode_applies_a_modifier_writes_its_handle_and_sees_it_end() {
    let blesser = r#"
fn on_input(ctx, player, name, value) {
    let hero = ctx.avatars()[0];
    if value == "bless" {
        let m = ctx.add_modifier(hero, "blessing", 100);
        ctx.state.seen = m.stacks;
        m.stacks = 3;
        m.state.count = m.state.count + 5;
    } else if value == "again" {
        ctx.state.seen = ctx.add_modifier(hero, "blessing").stacks;
    } else if value == "check" {
        ctx.state.seen = if hero.has_modifier("blessing") { 1 } else { 0 };
    } else if value == "twice" {
        let first = ctx.add_modifier(hero, "blessing");
        let second = ctx.add_modifier(hero, "blessing");
        ctx.state.seen = first.stacks * 10 + second.stacks;
    } else if value == "renew" {
        let m = ctx.add_modifier(hero, "blessing");
        m.state.count = 9;
        ctx.remove(m);
        let renewed = ctx.add_modifier(hero, "blessing");
        ctx.state.seen = renewed.stacks * 10 + renewed.state.count;
    } else if value == "unknown" {
        ctx.add_modifier(hero, "curse");
    }
}
"#;
    let mut game = Game::picking(blesser, ScriptLimits::ROOMY);
    let hero = game.pick(0, "hero-x");
    let held = |game: &Game| {
        let modifiers = game.sim.world.get::<Modifiers>(hero).unwrap();
        let clocks = game.sim.world.get::<ModifierClocks>(hero).unwrap();
        modifiers
            .iter()
            .enumerate()
            .map(|(at, instance)| {
                (
                    instance.stacks,
                    instance.lifetime.until().map(Tick::get),
                    clocks.state(at).to_vec(),
                )
            })
            .collect::<Vec<_>>()
    };
    // In tick t a new one, 1 stack as the call sees it, written to 3 and its count from 0 to 5;
    // 100 ms at 10 ticks a second is 1 tick, so it holds through t + 1 and ends as t + 2
    // starts.
    let t = game.sim.world.resource::<SimTick>().start().get();
    game.tick(&[(0, input("probe", "bless"))]);
    assert_eq!(game.field("seen"), StateValue::Int(1));
    assert_eq!(held(&game), [(3, Some(t + 2), vec![StateValue::Int(5)])]);
    // Again in t + 1 with its own 200 ms, 2 ticks: a fourth stack, the limit, and the end
    // t + 1 + 2 + 1; the count stays.
    game.tick(&[(0, input("probe", "again"))]);
    assert_eq!(game.field("seen"), StateValue::Int(4));
    assert_eq!(held(&game), [(4, Some(t + 4), vec![StateValue::Int(5)])]);
    game.tick(&[(0, input("probe", "check"))]);
    assert_eq!(game.field("seen"), StateValue::Int(1));
    // Through t + 3, then gone as t + 4 starts: once that tick has run.
    while game.sim.world.resource::<SimTick>().start().get() <= t + 4 {
        game.tick(&[]);
    }
    assert_eq!(held(&game), []);
    game.tick(&[(0, input("probe", "check"))]);
    assert_eq!(game.field("seen"), StateValue::Int(0));
    // Two applications in one call in tick u: one handle, which sees both, 2 stacks; 200 ms, so
    // it ends as u + 3 starts.
    let u = game.sim.world.resource::<SimTick>().start().get();
    game.tick(&[(0, input("probe", "twice"))]);
    assert_eq!(game.field("seen"), StateValue::Int(22));
    assert_eq!(held(&game), [(2, Some(u + 3), vec![StateValue::Int(0)])]);
    // In u + 1, a third stack written, removed, and applied again: a new one, 1 stack and its
    // count 0, which ends as u + 4 starts.
    game.tick(&[(0, input("probe", "renew"))]);
    assert_eq!(game.field("seen"), StateValue::Int(10));
    assert_eq!(held(&game), [(1, Some(u + 4), vec![StateValue::Int(0)])]);
    game.tick(&[(0, input("probe", "unknown"))]);
    assert_eq!(game.failures(), [Some(ApiError::UnknownModifier)]);
}

#[test]
fn a_player_modifier_holds_on_each_unit_of_its_player_it_selects_and_on_one_spawned_after() {
    let script = r#"
fn on_mode_input(ctx, player, name, value) {
    let at = ctx.map.markers("camp")[0].pos;
    if value == "units" {
        ctx.spawn_unit("grunt", "a", at, 1);
        ctx.spawn_unit("tower", "a", at, 1);
        ctx.spawn_unit("grunt", "a", at, 0);
    } else if value == "drill" {
        ctx.add_player_modifier(1, "drill");
    } else if value == "another" {
        ctx.spawn_unit("grunt", "a", at, 1);
    } else if value == "stranger" {
        ctx.add_player_modifier(1, "march");
    } else if value == "nobody" {
        ctx.add_player_modifier(9, "drill");
    }
}
"#;
    let mut game = Game::new(script, ScriptLimits::ROOMY);
    let drill = Stats::modifier(&game.sim.world, 0, "drill").unwrap();
    // Units 1 to 3, after the map's tower: a grunt and a tower of player 1, and a grunt of
    // player 0.
    game.tick(&[(1, input("probe", "units"))]);
    let drilled = |game: &Game| {
        let mut held = Vec::new();
        for (id, entity) in game.sim.world.resource::<EntityIndex>().iter() {
            let modifiers = game.sim.world.get::<Modifiers>(entity).unwrap();
            if let Some(instance) = modifiers.get(drill, None) {
                assert!(
                    instance.lifetime.held_by(Hold::Held) && instance.lifetime.until().is_none()
                );
                held.push(id.get());
            }
        }
        held
    };
    assert!(drilled(&game).is_empty());
    // From the tick player 1 holds the drill, its grunt holds it, from no source, with no end;
    // its tower is no grunt, and player 0's grunt is not its.
    game.tick(&[(1, input("probe", "drill"))]);
    assert_eq!(drilled(&game), [1]);
    // A grunt of player 1 spawned later holds it from its first Resolve.
    game.tick(&[(1, input("probe", "another"))]);
    assert_eq!(drilled(&game), [1, 4]);
    let held = game.sim.world.resource::<PlayerModifiers>();
    assert_eq!(held.of(PlayerSlot::new(1)).collect::<Vec<_>>(), [drill]);
    assert_eq!(held.of(PlayerSlot::new(0)).count(), 0);
    // A modifier the package does not declare, and a player the session does not have, fail.
    game.tick(&[
        (1, input("probe", "stranger")),
        (1, input("probe", "nobody")),
    ]);
    let refused = [ApiError::UnknownModifier, ApiError::UnknownPlayer];
    assert_eq!(game.failures(), refused.map(Some));
}
