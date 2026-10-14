use campfire_actions::{Actions, LaunchId};
use campfire_areas::*;
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_codec::Toml;
use campfire_common::{Tick, Ticks};
use campfire_deliveries::{Deliveries, Delivering};
use campfire_math::Num;
use campfire_mode::Mode;
use campfire_projectiles::Projectiles;
use campfire_script::Filter;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimComponent};
use campfire_stats::{ModifierData, Stats};
use campfire_units::{DeliveryEntity, UnitTypeBook, UnitTypeCore, Units};
use campfire_values::{
    DeclaredName, EngineTag, Number, ParamRef, Rank, Relation, RelationSet, TagSet, Team, TypeScope,
};

#[test]
fn a_filter_selects_a_delivery_unit_only_when_it_names_its_tag() {
    let types = UnitTypeBook::new(Release::engine_tags());
    let [projectile, area] = [Projectiles::TAG, Areas::TAG].map(EngineTag::tag);
    let parse = |text| Filter::parse(text, types.tags()).unwrap();
    // A relation alone is the filter of its name.
    assert_eq!(
        Filter::of_release_relations(RelationSet::Enemies),
        parse("enemies")
    );
    assert_eq!(Filter::of_release_relations(RelationSet::All), parse("all"));
    let units = [
        TagSet::default(),
        TagSet::from([projectile]),
        TagSet::from([area]),
    ];
    for (filter, selected) in [
        ("enemies", [true, false, false]),
        ("enemies:area", [false, false, true]),
        ("enemies:projectile", [false, true, false]),
        ("enemies:!area", [true, false, false]),
    ] {
        let selects = units.map(|tags| parse(filter).selects(Relation::Hostile, tags));
        assert_eq!(selects, selected, "{filter}");
    }
}

#[test]
fn an_area_reads_only_with_a_radius_that_is_not_negative() {
    let read = |text: &str| Toml::parse::<AreaData>(text);
    assert_eq!(
        read("radius = 0").unwrap(),
        AreaData {
            radius: Num::ZERO,
            delay_ms: 0,
            duration_ms: 0,
            affects: None,
            inside: AreaInsideData::default(),
        }
    );
    for refused in ["radius = -1", "radius = \"-0.5\""] {
        let message = read(refused).unwrap_err().message().to_owned();
        assert!(
            message.starts_with("an area's radius is not negative"),
            "{refused}"
        );
    }
}

#[test]
fn an_area_is_state_and_restores() {
    // A match of combat and areas with an area type and a train for its action, the same in both,
    // and a warding area type, which holds Rally on allies, reading its action's `ward`.
    let loads = |game: &mut Match| {
        let data = UnitTypeCore::default();
        let field = Units::load_type(game, TypeScope::Mode, "field", &data);
        let area = AreaData {
            radius: Num::int(2),
            delay_ms: 0,
            duration_ms: 0,
            affects: None,
            inside: AreaInsideData::default(),
        };
        Areas::load_type(game, field, 0, &area);
        let action = Actions::add_train(game, field, Ticks::ZERO, None);
        let ward = DeclaredName::new("ward").unwrap();
        let rally = ModifierData {
            shield: Some(Number::Param(ParamRef { param: ward })),
            ..ModifierData::default()
        };
        Stats::load_modifier(game, 0, "rally", &rally, None);
        let warding = Units::load_type(game, TypeScope::Mode, "warding", &data);
        let inside = AreaInsideData {
            allies: Some(DeclaredName::new("rally").unwrap()),
            ..AreaInsideData::default()
        };
        let area = AreaData { inside, ..area };
        Areas::load_type(game, warding, 0, &area);
        let source = Units::load_type(game, TypeScope::Mode, "source", &data);
        (field, action, warding, source)
    };
    let declared = [Capability::Stats, Capability::Combat, Capability::Areas];
    let mut game = Match::unscripted(&declared);
    let (field, action, warding, source_type) = loads(&mut game);
    let source = Mode::spawn(&mut game, source_type, Position::ORIGIN, Team::new(0), ());
    let by = Delivering {
        source,
        action,
        rank: Rank::FIRST,
        start: None,
        launch: None,
    };
    let area = Area::new(by, None, Tick::new(9));
    let trigger = AreaTrigger::at(Tick::new(5));
    let id = Deliveries::spawn_of(&mut game, source, field, Position::ORIGIN, (area, trigger));
    let mut restored = Match::unscripted(&declared);
    loads(&mut restored);
    Release::restore_into(&game, &mut restored);
    assert_eq!(restored.part::<Area>(id), game.part::<Area>(id));
    assert_eq!(restored.part::<AreaTrigger>(id), &trigger);
    // It triggers by its end, at tick 9 at the latest; a unit that is no area has no trigger.
    let entity = game.unit_entity(id);
    let triggers = |at| AreaTrigger::at(Tick::new(at)).check(&game, entity);
    assert_eq!([5, 9, 10].map(triggers), [true, true, false]);
    let id = Mode::spawn(&mut game, source_type, Position::ORIGIN, Team::new(0), ());
    assert!(!trigger.check(&game, game.unit_entity(id)));
    // Its times are at most the limit: a tick past it fails to decode.
    let past = Tick::new(Tick::LIMIT.get() + 1);
    assert!(Match::decodes(&Area::new(by, None, Tick::LIMIT)));
    assert!(!Match::decodes(&Area::new(by, None, past)));
    assert!(Match::decodes(&AreaTrigger::at(Tick::LIMIT)));
    assert!(!Match::decodes(&AreaTrigger::at(past)));
    // An area of a launch the match did not load has no lists to run.
    let launched = Delivering {
        launch: Some(LaunchId::nth(0)),
        ..by
    };
    let area = Area::new(launched, None, Tick::LIMIT);
    assert!(!area.check(&game, entity));
    // A warding area: the train, which declares no `ward`, cannot have delivered it, as Rally
    // would fail as an ally takes it.
    let warded = Deliveries::spawn_of(&mut game, source, warding, Position::ORIGIN, DeliveryEntity);
    let area = Area::new(by, None, Tick::LIMIT);
    assert!(area.check(&game, entity));
    assert!(!area.check(&game, game.unit_entity(warded)));
}
