use super::*;

#[test]
fn player_inputs_choose_heroes_and_spells_and_a_failed_call_changes_nothing() {
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    let [strike, blink] = ["strike", "blink"].map(|name| Actions::named(&game, 0, name).unwrap());
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
    Mode::tick_inputs(
        &mut game,
        &[
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
        ],
    );
    // The failed calls count no input and spawn nothing: three calls succeeded, player 0's two
    // and player 2's choice of the other hero. The thrown call fails with no refusal of the API,
    // and the id it took for its grunt is free again.
    assert_eq!(Mode::field(&game, "inputs"), StateValue::Int(3));
    assert_eq!(
        Mode::field(&game, "phase"),
        StateValue::Text("start".into())
    );
    assert_eq!(
        Mode::failures(&game),
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
    let chosen = game.resource::<Choices>().offers();
    let rows: Vec<_> = chosen.chunks(4).collect();
    assert_eq!(
        rows,
        [
            [None, None, offer(0), offer(0)],
            [None; 4],
            [None, None, offer(1), None],
        ]
    );
    let next = game.resource_mut::<IdAllocator>().allocate();
    assert_eq!(next.get(), 7);
    // The heroes, 5 and 6, at their teams' spawns under their players' control: player 0's
    // with its own ability unlearned, then its spell learned.
    let heroes: Vec<_> = Mode::placed_units(&game)[5..].to_vec();
    assert_eq!(
        heroes,
        [
            (5, Position::from((0, 0, -5)), 0, None),
            (6, Position::from((0, 0, 5)), 1, None)
        ]
    );
    let hero = Mode::entity_by_id(&game, 5);
    assert_eq!(game.get::<Owner>(hero).unwrap().slot(), PlayerSlot::new(0));
    let slots = game.get::<ActionSlots>(hero).unwrap();
    let slots: Vec<_> = slots
        .iter()
        .map(|slot| (slot.action.unwrap(), Rank::count(slot.rank)))
        .collect();
    assert_eq!(slots, [(strike, 0), (blink, 1)]);
}

#[test]
fn resources_add_up_and_queries_see_teams_paths_and_the_dead() {
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    Mode::tick_inputs(
        &mut game,
        &[(0, input("hero", "hero-x")), (2, input("hero", "hero-y"))],
    );
    // Hero Y carries its passive, the blessing, from itself, with no end.
    let mut owned = game.query::<(&StableId, &Owner, &Modifiers)>();
    let (&hero_y, _, modifiers) = owned
        .iter(&game)
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
    let grunt = Mode::entity_by_id(&game, 3);
    game.entity_mut(grunt).insert(Dead);
    Mode::tick_inputs(
        &mut game,
        &[
            (1, input("gold", "gold")),
            (1, input("rich", "gems")),
            (1, input("rich", "gems")),
            (1, input("gold", "silver")),
            (0, input("probe", "")),
        ],
    );
    // Gold, the first of the mode's resources, and gems, the second; a resource the mode does
    // not declare fails, as a sum past what an integer holds does.
    let amount = |slot, name| {
        let resources = game.player_resources(PlayerSlot::new(slot));
        resources.unwrap().amount(resource(name).unwrap())
    };
    assert_eq!(
        [amount(1, "gold"), amount(1, "gems"), amount(0, "gold")],
        [16, i64::MAX, 0]
    );
    assert_eq!(resource("gems").map(ResourceId::index), Some(1));
    assert_eq!(
        Mode::failures(&game),
        [
            FailureKind::Api(ApiError::ResourceOverflow),
            FailureKind::Api(ApiError::Unknown(NameKind::Resource))
        ]
    );
    // The enemy of a, the 4 grunts with the dead one, b's one hero, the 2 playing teams, the 3
    // players, the path and team of grunt 2, the neutral grunt 1's team, hero 5's owner, the path
    // of the tower, which stands on it as grunt 2 walks it, and grunt 2's unit type.
    let text = |text: &str| StateValue::Text(text.into());
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
    .map(|name| Mode::field(&game, name));
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
    let mut game = picking_match(script, ScriptLimits::ROOMY);
    let [strike, blink] = ["strike", "blink"].map(|name| Actions::named(&game, 0, name).unwrap());
    let probe = |value| input("probe", value);
    let read = |game: &Match| ["kind", "seen", "team"].map(|name| Mode::field(game, name));
    let text = |text: &str| StateValue::Text(text.into());
    // Before any choice: no duo, hero X free to player 0, who is on team a.
    Mode::tick_inputs(&mut game, &[(0, probe("read"))]);
    assert_eq!(read(&game), [text("none"), StateValue::Int(1), text("a")]);
    // Player 0 takes hero X, and spawns it with no spells, as it chose none: player 2, of team
    // b, may not take it, as the hero choice is unique.
    Mode::tick_inputs(
        &mut game,
        &[(0, input("hero", "hero-x")), (2, probe("read"))],
    );
    assert_eq!(read(&game), [text("none"), StateValue::Int(0), text("b")]);
    // Unit 1, after the map's tower.
    let hero = Mode::entity_by_id(&game, 1);
    let slots = |game: &Match| {
        let slots = game.get::<ActionSlots>(hero).unwrap();
        let slots = slots
            .iter()
            .map(|slot| (slot.action.unwrap(), slot.kind, slot.rank));
        slots.collect::<Vec<_>>()
    };
    let [basic, spell] = [0, 1].map(SlotKind::new);
    assert_eq!(slots(&game), [(strike, basic, None)]);
    // A choice not unique: players 1 and 2 both take both heroes, in their own order; then
    // player 1 chooses again, which replaces its values. Too few values, one twice, one the
    // choice does not offer and a choice the mode does not declare fail, and change nothing.
    Mode::tick_inputs(
        &mut game,
        &[
            (1, probe("duo")),
            (2, probe("duo")),
            (1, probe("swap")),
            (1, probe("short")),
            (1, probe("twice")),
            (1, probe("stranger")),
            (1, probe("nothing")),
            (1, probe("read")),
        ],
    );
    let refused = [
        ApiError::ChoiceCount,
        ApiError::RepeatedChoiceValue,
        ApiError::UnknownChoiceValue,
        ApiError::Unknown(NameKind::Choice),
    ];
    assert_eq!(Mode::failures(&game), refused.map(FailureKind::Api));
    assert_eq!(Mode::field(&game, "kind"), text("hero-x,hero-y"));
    Mode::tick_inputs(&mut game, &[(2, probe("read"))]);
    assert_eq!(Mode::field(&game, "kind"), text("hero-y,hero-x"));
    // A grant puts the spell after the hero's basic ability, learned as its kind has no ranks;
    // a kind of other ranks, a kind the mode does not declare and an action that is no loadout
    // entry fail.
    Mode::tick_inputs(
        &mut game,
        &[
            (0, probe("grant")),
            (0, probe("grant_basic")),
            (0, probe("grant_ultimate")),
            (0, probe("grant_stranger")),
        ],
    );
    let refused = [
        ApiError::SlotKindRanks,
        ApiError::Unknown(NameKind::SlotKind),
        ApiError::Unknown(NameKind::Ability),
    ];
    assert_eq!(Mode::failures(&game), refused.map(FailureKind::Api));
    assert_eq!(
        slots(&game),
        [(strike, basic, None), (blink, spell, Some(Rank::FIRST))]
    );
}

#[test]
fn a_grant_to_a_fresh_unit_counts_its_inventory_slots() {
    // Hero Y carries 255 inventory slots of the spell kind and no action. Granted two spells as it
    // spawns, it would hold 257 slots, past the 256 a unit holds: the grant fails, and its call's
    // spawn with it. Granted one, it holds 256, the spell after the inventory's slots.
    let script = r#"
fn on_input(ctx, player, name, value) {
    let id = ctx.spawn_unit("hero-y", "a", ctx.map.markers("camp")[0].pos, player);
    ctx.grant(id, "spell", if value == "two" { ["blink", "blink"] } else { ["blink"] });
}
"#;
    let spell = SlotKind::new(1);
    let carry = |types: &mut [TestType]| {
        types[2].inventory = Some((Inventory::new(NonZeroU8::MAX), UseGroup::new(spell)));
    };
    let script = format!("{script}{PICKING}");
    let mut game = start_match_setup(&script, ScriptLimits::ROOMY, &mode_files(), carry).unwrap();
    let blink = Actions::named(&game, 0, "blink").unwrap();
    let owned = |game: &mut Match| {
        let mut owned = game.query::<(&Owner, &ActionSlots)>();
        let slots = owned.iter(game).map(|(_, slots)| slots.clone());
        slots.collect::<Vec<_>>()
    };
    Mode::tick_inputs(&mut game, &[(0, input("probe", "two"))]);
    assert_eq!(
        Mode::failures(&game),
        [FailureKind::Api(ApiError::TooManySlots)]
    );
    assert!(owned(&mut game).is_empty());
    Mode::tick_inputs(&mut game, &[(0, input("probe", "one"))]);
    assert_eq!(Mode::failures(&game), []);
    let [slots] = owned(&mut game).try_into().unwrap();
    assert_eq!(slots.len(), ActionSlots::LIMIT);
    let last = slots.slot(255).unwrap();
    assert_eq!(
        (last.action, last.kind, last.rank),
        (Some(blink), spell, Some(Rank::FIRST))
    );
    assert!(
        slots
            .iter()
            .take(255)
            .all(|slot| slot.action.is_none() && slot.kind == spell)
    );
}

#[test]
fn a_players_avatar_is_the_unit_it_owns_of_an_avatar_type() {
    // Player 0 picks hero X, of an avatar's type, and owns a grunt besides; player 1 owns only a
    // grunt, and player 2 nothing. A second hero of player 0 of a higher id is not its avatar.
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    let hero = Mode::pick(&mut game, 0, "hero-x");
    let grunt = *game.get::<UnitType>(Mode::entity_by_id(&game, 1)).unwrap();
    let hero_type = *game.get::<UnitType>(hero).unwrap();
    let pos = Position::new(Vec3::new(Num::ZERO, Num::ZERO, Num::ZERO)).unwrap();
    let [_, grunt_1, second] = [(0, grunt), (1, grunt), (0, hero_type)].map(|(slot, unit_type)| {
        let owner = Owner::new(PlayerSlot::new(slot));
        Mode::spawn(&mut game, unit_type, pos, Team::new(0), owner)
    });
    let mut players = HeldPlayerUnits::new(&mut game);
    let avatar = |players: &mut HeldPlayerUnits, slot| {
        players
            .avatar(&game, PlayerSlot::new(slot))
            .map(|avatar| avatar.entity)
    };
    assert_eq!(avatar(&mut players, 0), Some(hero));
    assert_eq!(avatar(&mut players, 1), None);
    assert_eq!(avatar(&mut players, 2), None);
    // The world's own read, for code that holds it shared, finds each the same.
    let shared = (0..3).map(|slot| {
        game.avatar_of(PlayerSlot::new(slot))
            .map(|avatar| avatar.entity)
    });
    assert_eq!(shared.collect::<Vec<_>>(), [Some(hero), None, None]);

    // Out of play, as each tick ends: player 0 while both its heroes are dead, and no more as one
    // lives again; player 1, whose grunt is no avatar, never by a death, and while the script
    // holds it out; player 2, of no unit, never.
    let second = Mode::entity_by_id(&game, second.get());
    let out = |game: &Match| {
        (0..3)
            .map(|slot| {
                let player = game.player_entity(PlayerSlot::new(slot)).unwrap();
                game.entity(player).contains::<OutOfPlay>()
            })
            .collect::<Vec<bool>>()
    };
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(out(&game), [false, false, false]);
    game.entity_mut(hero).insert(Dead);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(out(&game), [false, false, false]);
    game.entity_mut(second).insert(Dead);
    let grunt_1 = Mode::entity_by_id(&game, grunt_1.get());
    game.entity_mut(grunt_1).insert(Dead);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(out(&game), [true, false, false]);
    game.entity_mut(hero).remove::<Dead>();
    Mode::tick_inputs(&mut game, &[(1, input("out", "on"))]);
    assert_eq!(out(&game), [false, true, false]);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(out(&game), [false, true, false]);
    Mode::tick_inputs(&mut game, &[(1, input("out", "off"))]);
    assert_eq!(out(&game), [false, false, false]);
}
