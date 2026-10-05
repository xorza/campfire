use super::*;

#[test]
fn player_inputs_choose_heroes_and_spells_and_a_failed_call_changes_nothing() {
    let mut game = Game::new(SCRIPT, ScriptLimits::ROOMY);
    let spells = ModeInput {
        name: "spells",
        value: InputValue::StringList(vec!["blink"]),
    };
    let twice = ModeInput {
        name: "spells",
        value: InputValue::StringList(vec!["blink", "blink"]),
    };
    let wrong_type = ModeInput {
        name: "hero",
        value: InputValue::StringList(vec!["hero-x"]),
    };
    game.tick(&[
        (0, spells),
        (0, input("hero", "hero-x")),
        // Taken by player 0.
        (2, input("hero", "hero-x")),
        (2, twice),
        (2, input("hero", "hero-y")),
        // Neither of the mode's inputs: never a call.
        (2, input("nope", "x")),
        (2, wrong_type),
        (1, input("fail", "boom")),
        (1, input("phase", "")),
    ]);
    // The failed calls count no input and spawn nothing: three calls succeeded, player 0's two
    // and player 2's choice of the other hero. The thrown call fails with no refusal of the API,
    // and the id it took for its grunt is free again.
    assert_eq!(game.field("inputs"), StateValue::Int(3));
    assert_eq!(game.field("phase"), StateValue::Text("start".to_owned()));
    assert_eq!(
        game.failures(),
        [
            FailureKind::Api(ApiError::ChoiceTaken),
            FailureKind::Api(ApiError::ChoiceCount),
            FailureKind::Raised(None),
            FailureKind::Api(ApiError::WrongStateType)
        ]
    );
    // Each player's row: duo's two values, hero's, spells', the choices by name. Player 0 chose
    // hero X, offer 0, and blink, the one spell; player 2 hero Y, offer 1.
    let offer = |index| Some(Offer::new(index));
    let chosen = &game.sim.world.resource::<Choices>().0;
    let rows: Vec<_> = chosen.chunks(4).collect();
    assert_eq!(
        rows,
        [
            [None, None, offer(0), offer(0)],
            [None; 4],
            [None, None, offer(1), None],
        ]
    );
    let next = game.sim.world.resource_mut::<IdAllocator>().allocate();
    assert_eq!(next.get(), 7);
    // The heroes, 5 and 6, at their teams' spawns under their players' control: player 0's
    // with its own ability unlearned, then its spell learned.
    let heroes: Vec<_> = game.units()[5..].to_vec();
    assert_eq!(heroes, [(5, at(0, -5), 0, None), (6, at(0, 5), 1, None)]);
    let hero = game.entity(5);
    assert_eq!(
        game.sim.world.get::<Owner>(hero).unwrap().slot(),
        PlayerSlot::new(0)
    );
    let slots = game.sim.world.get::<ActionSlots>(hero).unwrap();
    let slots: Vec<_> = slots
        .iter()
        .map(|slot| (slot.action.unwrap(), slot.rank))
        .collect();
    assert_eq!(slots, [(game.strike, 0), (game.blink, 1)]);
}

#[test]
fn resources_add_up_and_queries_see_teams_paths_and_the_dead() {
    let mut game = Game::new(SCRIPT, ScriptLimits::ROOMY);
    game.tick(&[(0, input("hero", "hero-x")), (2, input("hero", "hero-y"))]);
    // Hero Y carries its passive, the blessing, from itself, with no end.
    let mut owned = game.sim.world.query::<(&StableId, &Owner, &Modifiers)>();
    let (&hero_y, _, modifiers) = owned
        .iter(&game.sim.world)
        .find(|(_, owner, _)| owner.slot() == PlayerSlot::new(2))
        .unwrap();
    let held: Vec<_> = modifiers
        .iter()
        .map(|instance| {
            (
                instance.source,
                instance.lifetime.held_by(Hold::Passive),
                instance.lifetime.until(),
                instance.stacks,
            )
        })
        .collect();
    assert_eq!(held, [(Some(hero_y), true, None, 1)]);
    // A dead grunt is still one the mode sees.
    let grunt = game.entity(3);
    game.sim.world.entity_mut(grunt).insert(Dead);
    game.tick(&[
        (1, input("gold", "gold")),
        (1, input("rich", "gems")),
        (1, input("rich", "gems")),
        (1, input("gold", "silver")),
        (0, input("probe", "")),
    ]);
    // Gold, the first of the mode's resources, and gems, the second; a resource the mode does
    // not declare fails, as a sum past what an integer holds does.
    let resources = game.sim.world.resource::<PlayerResources>();
    let amount = |slot, name| {
        let resource = resource(name).unwrap();
        resources.amount(PlayerSlot::new(slot), resource)
    };
    assert_eq!(
        [amount(1, "gold"), amount(1, "gems"), amount(0, "gold")],
        [16, i64::MAX, 0]
    );
    assert_eq!(resource("gems").map(ResourceId::index), Some(1));
    assert_eq!(
        game.failures(),
        [
            FailureKind::Api(ApiError::ResourceOverflow),
            FailureKind::Api(ApiError::UnknownResource)
        ]
    );
    // The enemy of a, the 4 grunts with the dead one, b's one hero, the 2 playing teams, the 3
    // players, the path and team of grunt 2, the neutral grunt 1's team, hero 5's owner, the path
    // of the tower, which stands on it as grunt 2 walks it, and grunt 2's unit type.
    let text = |text: &str| StateValue::Text(text.to_owned());
    let seen = [
        "enemy",
        "grunts",
        "heroes",
        "teams",
        "players",
        "path",
        "team",
        "neutral",
        "owner",
        "tower_path",
        "kind",
    ]
    .map(|name| game.field(name));
    assert_eq!(
        seen,
        [
            text("b"),
            StateValue::Int(4),
            StateValue::Int(1),
            StateValue::Int(2),
            StateValue::Int(3),
            text("mid"),
            text("a"),
            text("neutral"),
            StateValue::Int(0),
            text("mid"),
            text("grunt"),
        ]
    );
}

#[test]
fn choices_hold_each_players_values_and_grants_fill_a_slot_kind() {
    let script = r#"
fn on_input(ctx, player, name, value) {
    if value == "read" {
        let duo = ctx.chosen(player, "duo");
        ctx.state.kind = if duo.is_empty() { "none" } else { duo[0] + "," + duo[1] };
        ctx.state.seen = if ctx.available(player, "hero", "hero-x") { 1 } else { 0 };
        ctx.state.team = ctx.team_of(player);
    } else if value == "duo" {
        ctx.choose(player, "duo", ["hero-y", "hero-x"]);
    } else if value == "swap" {
        ctx.choose(player, "duo", ["hero-x", "hero-y"]);
    } else if value == "short" {
        ctx.choose(player, "duo", ["hero-y"]);
    } else if value == "twice" {
        ctx.choose(player, "duo", ["hero-x", "hero-x"]);
    } else if value == "stranger" {
        ctx.choose(player, "duo", ["hero-x", "hero-z"]);
    } else if value == "nothing" {
        ctx.choose(player, "nothing", "hero-x");
    } else if value == "grant" {
        ctx.grant(ctx.avatars()[0], "spell", ["blink"]);
    } else if value == "grant_basic" {
        ctx.grant(ctx.avatars()[0], "basic", ["blink"]);
    } else if value == "grant_ultimate" {
        ctx.grant(ctx.avatars()[0], "ultimate", ["blink"]);
    } else if value == "grant_stranger" {
        ctx.grant(ctx.avatars()[0], "spell", ["haste"]);
    }
}
"#;
    let mut game = Game::picking(script, ScriptLimits::ROOMY);
    let probe = |value| input("probe", value);
    let read = |game: &Game| ["kind", "seen", "team"].map(|name| game.field(name));
    let text = |text: &str| StateValue::Text(text.to_owned());
    // Before any choice: no duo, hero X free to player 0, who is on team a.
    game.tick(&[(0, probe("read"))]);
    assert_eq!(read(&game), [text("none"), StateValue::Int(1), text("a")]);
    // Player 0 takes hero X, and spawns it with no spells, as it chose none: player 2, of team
    // b, may not take it, as the hero choice is unique.
    game.tick(&[(0, input("hero", "hero-x")), (2, probe("read"))]);
    assert_eq!(read(&game), [text("none"), StateValue::Int(0), text("b")]);
    // Unit 1, after the map's tower.
    let hero = game.entity(1);
    let slots = |game: &Game| {
        let slots = game.sim.world.get::<ActionSlots>(hero).unwrap();
        let slots = slots
            .iter()
            .map(|slot| (slot.action.unwrap(), slot.kind, slot.rank));
        slots.collect::<Vec<_>>()
    };
    let [basic, spell] = [0, 1].map(SlotKind::new);
    assert_eq!(slots(&game), [(game.strike, basic, 0)]);
    // A choice not unique: players 1 and 2 both take both heroes, in their own order; then
    // player 1 chooses again, which replaces its values. Too few values, one twice, one the
    // choice does not offer and a choice the mode does not declare fail, and change nothing.
    game.tick(&[
        (1, probe("duo")),
        (2, probe("duo")),
        (1, probe("swap")),
        (1, probe("short")),
        (1, probe("twice")),
        (1, probe("stranger")),
        (1, probe("nothing")),
        (1, probe("read")),
    ]);
    let refused = [
        ApiError::ChoiceCount,
        ApiError::RepeatedChoiceValue,
        ApiError::UnknownChoiceValue,
        ApiError::UnknownChoice,
    ];
    assert_eq!(game.failures(), refused.map(FailureKind::Api));
    assert_eq!(game.field("kind"), text("hero-x,hero-y"));
    game.tick(&[(2, probe("read"))]);
    assert_eq!(game.field("kind"), text("hero-y,hero-x"));
    // A grant puts the spell after the hero's basic ability, learned as its kind has no ranks;
    // a kind of other ranks, a kind the mode does not declare and an action that is no loadout
    // entry fail.
    game.tick(&[
        (0, probe("grant")),
        (0, probe("grant_basic")),
        (0, probe("grant_ultimate")),
        (0, probe("grant_stranger")),
    ]);
    let refused = [
        ApiError::SlotKindRanks,
        ApiError::UnknownSlotKind,
        ApiError::UnknownAction,
    ];
    assert_eq!(game.failures(), refused.map(FailureKind::Api));
    assert_eq!(
        slots(&game),
        [(game.strike, basic, 0), (game.blink, spell, 1)]
    );
}
