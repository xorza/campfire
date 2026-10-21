use campfire_actions::ProgressDues;
use campfire_navigation::{PathArrival, PathArrivals};
use campfire_production::{
    CompletedSites, Gatherer, Load, Node, ProductionRefusal, Site, SiteProgress, SiteStart,
};
use campfire_script::CapabilityRefusal;
use campfire_sim::Position;
use campfire_sim::internals::SimWorldInternals;

use super::*;

#[test]
fn a_train_pays_at_once_joins_the_queue_and_spawns_its_unit_when_its_time_ends() {
    let picker = "
fn on_input(ctx, player, name, value) {}

fn on_unit_trained(ctx, unit, producer) {
    ctx.state.kind += `${unit.unit_type} of ${producer.unit_type}, then ${producer.queue};`;
}
";
    // A barracks of player 0, with 100 mana and a queue of 2, trains a grunt for 30 mana and 5
    // gold, in 300 ms, 3 ticks at 10 a second; its slot of the spell kind holds the train from
    // its spawn.
    let mut set = mode_packages(picker);
    set.data.push_str(
        "\n[actions.train_grunt]\nkind = \"train\"\ntargeting = \"none\"\n\
         cost = { mana = 30, gold = 5 }\nwindup_ms = 300\nunit_type = \"grunt\"\n",
    );
    set.units.push_str(&stat_type("barracks", &[]));
    set.units
        .push_str("production = { queue = 2 }\nslots = { spell = [\"train_grunt\"] }\n");
    let mut game = started(&set, ScriptLimits::ROOMY).unwrap();
    let owner = Owner::new(Seat::new(0));
    let barracks = Spawn::named(&game, "barracks", Vec3::ZERO).parts(owner);
    let barracks = barracks.place(&mut game);
    let producer = game.unit_entity(barracks);
    let barracks_type = *game.get::<UnitType>(producer).unwrap();
    Mode::tick_inputs(&mut game, &[]);
    let gold = resource("gold").unwrap();
    let player = Seat::new(0);
    let mut resources = game.player_resources_mut(player).unwrap();
    resources.add(gold, 15).unwrap();
    let order = |game: &mut Match| {
        let id = *game.get::<StableId>(producer).unwrap();
        ProgressDues::change(game, id, producer, |progress, dues| {
            progress.order(0, ActionTarget::None, dues);
        })
        .unwrap();
    };
    let paid = |game: &Match| {
        let mana = game.get::<Pools>(producer).unwrap().current(MANA).unwrap();
        let held = game.player_resources(player).unwrap().amount(gold);
        (mana.round(), held)
    };
    let mut grunts = game.query_filtered::<(&UnitType, &Owner, &Position), With<Owner>>();
    let mut trained = |game: &mut Match| {
        let units = grunts.iter(game);
        units
            .filter(|&(&unit_type, ..)| unit_type != barracks_type)
            .count()
    };

    // Tick 0 ran. Ticks 1 and 2 each order a train: each pays 30 mana and 5 gold at once.
    // The first's 3 ticks run from time 1 to time 4, the end of tick 3, so its unit spawns in tick
    // 3's Mode stage; the second's run from then to time 7, tick 6. A third, ordered in tick 3's
    // Act stage, before the first spawns, finds the queue full: nothing is paid.
    let steps = [
        (true, (70, 10), 0),
        (true, (40, 5), 0),
        (true, (40, 5), 1),
        (false, (40, 5), 1),
        // Room again: a third pays, and runs from time 7 to time 10, the end of tick 9.
        (true, (10, 0), 1),
        (false, (10, 0), 2),
        (false, (10, 0), 2),
        // Room again, and 10 mana of 30: the order fails, and nothing is paid.
        (true, (10, 0), 2),
        (false, (10, 0), 3),
        (false, (10, 0), 3),
    ];
    // Each unit trained runs `on_unit_trained` in the tick it spawns, with its producer, whose
    // queue then holds the trains behind it: the second's, the third's, then none.
    let calls = |made| {
        let call = |behind| format!("grunt of barracks, then {behind};");
        let calls = [call(r#"["grunt"]"#), call(r#"["grunt"]"#), call("[]")];
        StateValue::Text(calls[..made].concat().into())
    };
    for (at, (ordered, expected, made)) in (1..).zip(steps) {
        assert_eq!(game.now().get(), at);
        if ordered {
            order(&mut game);
        }
        Mode::tick_inputs(&mut game, &[]);
        assert_eq!(paid(&game), expected, "tick {at}");
        assert_eq!(trained(&mut game), made, "tick {at}");
        assert_eq!(Mode::field(&game, "kind"), calls(made), "tick {at}");
    }
    // Each grunt stands where the producer stood, its player's, and the queue is empty.
    let position = game.get::<Position>(producer).unwrap().get();
    let made: Vec<_> = grunts
        .iter(&game)
        .filter(|&(&unit_type, ..)| unit_type != barracks_type)
        .map(|(_, owner, grunt_pos)| (owner.seat(), grunt_pos.get()))
        .collect();
    assert_eq!(made, [(player, position); 3]);
    assert!(
        game.get::<TrainQueue>(producer)
            .unwrap()
            .entries()
            .is_empty()
    );
    assert_eq!(Mode::failures(&game), []);
}

#[test]
fn a_script_reads_each_players_supply_and_each_units_load_as_the_view_read_the_match() {
    let script = r#"
fn on_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
    } else if value == "load" {
        ctx.state.count = 0;
        for unit in ctx.all_units_tagged("grunt") {
            ctx.state.count = ctx.state.count * 100 + unit.load;
        }
    } else if value == "built" {
        ctx.state.kind = "";
        for unit in ctx.all_units_tagged("grunt") {
            ctx.state.kind += `${unit.site_progress ?? "-"} ${unit.node_amount ?? "-"};`;
        }
    } else {
        ctx.state.count = player.supply_used;
        ctx.state.seen = player.supply_cap;
    }
}
"#;
    // Hero X uses 2 and gives 6, of a most of 5; a grunt uses 1.
    let mut set = mode_packages(script);
    set.data.push_str("\n[supply]\nmax = 5\n");
    let hero_slots = "slots = { basic = [\"strike\"] }\n";
    let hero_x = &mut set.avatars[0].data;
    let supplied = format!("{hero_slots}supply = {{ cost = 2, provides = 6 }}\n");
    *hero_x = replaced(hero_x, hero_slots, &supplied);
    let grunt_death = "combat = { on_death = \"stay\" }\nstate";
    let used = "combat = { on_death = \"stay\" }\nsupply = { cost = 1, provides = 0 }\nstate";
    set.units = replaced(&set.units, grunt_death, used);
    let mut game = started(&set, ScriptLimits::ROOMY).unwrap();
    let grunt = Units::type_named(&game, "grunt").unwrap();
    Mode::tick_inputs(&mut game, &[(0, input("hero", "hero-x"))]);
    // Player 0 owns three grunts, one of them dead; player 1 one. The second carries 7 gold, the
    // third gathers and carries nothing.
    let gold = resource("gold").unwrap();
    let loads = [None, Some(7), Some(0), None];
    let mut grunts = Vec::new();
    for ((slot, dead), load) in [(0, false), (0, false), (0, true), (1, false)]
        .into_iter()
        .zip(loads)
    {
        let position = Vec3::ZERO;
        let owner = Owner::new(Seat::new(slot));
        let id = Spawn::of(grunt, position).parts(owner).place(&mut game);
        grunts.push(id);
        if dead {
            game.insert_parts(id, Dead);
        }
        if let Some(amount) = load {
            let mut gatherer = Gatherer::default();
            gatherer.carry((amount > 0).then_some(Load {
                resource: gold,
                amount,
            }));
            game.insert_parts(id, gatherer);
        }
    }
    // Player 0 uses 2 + 1 + 1 = 4 of a cap of 6 at most 5; player 1 uses 1 of none.
    Mode::tick_inputs(&mut game, &[(0, input("phase", "read"))]);
    assert_eq!(
        (Mode::field(&game, "count"), Mode::field(&game, "seen")),
        (StateValue::Int(4), StateValue::Int(5))
    );
    Mode::tick_inputs(&mut game, &[(1, input("phase", "read"))]);
    assert_eq!(
        (Mode::field(&game, "count"), Mode::field(&game, "seen")),
        (StateValue::Int(1), StateValue::Int(0))
    );
    // The loads by stable id, 0, 7, 0 and 0, two digits each: ((0·100 + 7)·100 + 0)·100 + 0.
    Mode::tick_inputs(&mut game, &[(0, input("phase", "load"))]);
    assert_eq!(Mode::field(&game, "count"), StateValue::Int(70_000));
    // The first grunt is a site of 4 ticks, 1 of them done, a share of 1/4; the last a node of
    // 25; the others neither.
    let site = Site::new(SiteStart {
        action: ActionId::new(0),
        rank: Rank::FIRST,
        time: Num::int(4),
        gain: Num::ZERO,
        paid: &[],
        holder: None,
    });
    let mut progress = SiteProgress::default();
    progress.grow(&site, Num::ONE);
    game.insert_parts(grunts[0], (site, progress));
    game.insert_parts(grunts[3], Node::new(25));
    Mode::tick_inputs(&mut game, &[(0, input("phase", "built"))]);
    assert_eq!(
        Mode::field(&game, "kind"),
        StateValue::Text("0.25 -;- -;- -;- 25;".into())
    );
    assert_eq!(Mode::failures(&game), []);
    // A mode that counts no supply fails the call.
    let mut uncounted = mode_match(script, ScriptLimits::ROOMY);
    Mode::tick_inputs(&mut uncounted, &[(0, input("phase", "read"))]);
    assert_eq!(
        Mode::failures(&uncounted),
        [FailureKind::Api(ProductionRefusal::NoSupply.refused())]
    );
}

#[test]
fn completed_sites_then_walkers_at_their_paths_ends_run_their_hooks_and_a_gone_unit_none() {
    let script = "
fn on_input(ctx, player, name, value) {}

fn on_site_complete(ctx, unit) {
    ctx.state.kind += `site ${unit.unit_type};`;
}

fn on_path_end(ctx, unit, path) {
    ctx.state.kind += `${unit.unit_type} at ${path};`;
}
";
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    Mode::tick_inputs(&mut game, &[]);
    let named = |name: &str| {
        let names = game.resource::<MatchNames>();
        names.named((TypeScope::Mode, name)).unwrap()
    };
    let grunt = named("grunt");
    let tower = named("tower");
    let walker = Spawn::of(grunt, Vec3::ZERO).place(&mut game);
    let built = Spawn::of(tower, Vec3::ZERO).place(&mut game);
    let gone = Spawn::of(grunt, Vec3::ZERO).place(&mut game);
    let entity = game.unit_entity(gone);
    game.despawn(entity);
    // The walker's arrival joins before the site's, and the gone unit's of each runs no call: the
    // sites run first all the same.
    let mid = PathId::new(0);
    game.resource_mut::<PathArrivals>()
        .0
        .extend([walker, gone].map(|id| PathArrival { id, path: mid }));
    game.resource_mut::<CompletedSites>()
        .0
        .extend([gone, built]);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(
        Mode::field(&game, "kind"),
        StateValue::Text("site tower;grunt at mid;".into())
    );
    assert!(game.resource::<PathArrivals>().0.is_empty());
    assert!(game.resource::<CompletedSites>().0.is_empty());
    assert_eq!(Mode::failures(&game), []);
}
