use campfire_codec::Toml;
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_sim::internals::SimWorldInternals;

use super::*;
use crate::actions::Actions;
use crate::actions::launch_id::LaunchId;
use crate::areas::area_data::{AreaData, AreaInsideData};
use crate::projectiles::Projectiles;
use crate::release::Release;
use crate::release::r#match::Match;
use crate::stats::Stats;
use crate::stats::modifier_data::ModifierData;
use crate::units::Units;
use crate::units::filter::Filter;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type_core::UnitTypeCore;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::number::{Number, ParamRef};
use crate::values::rank::Rank;
use crate::values::relation::Relation;
use crate::values::relation_set::RelationSet;

#[test]
fn a_filter_selects_a_delivery_unit_only_when_it_names_its_tag() {
    let types = UnitTypes::new(Release::engine_tags());
    let [projectile, area] = [Projectiles::TAG, Areas::TAG].map(EngineTag::tag);
    let parse = |text| Filter::parse(text, &types).unwrap();
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
        (field, action, warding)
    };
    let declared = [Capability::Stats, Capability::Combat, Capability::Areas];
    let mut game = Match::client(&declared);
    let (field, action, warding) = loads(&mut game);
    let source = Units::spawn(&mut game, Position::ORIGIN, Team::new(0), ());
    let by = Delivering {
        source,
        action,
        rank: Rank::FIRST,
        start: None,
        launch: None,
    };
    let area = Area::new(by, None, Tick::new(9));
    let trigger = AreaTrigger::at(Tick::new(5));
    let id = Units::spawn_delivery_of(
        &mut game,
        field,
        Position::ORIGIN,
        Team::new(0),
        (area, trigger),
    );
    let mut restored = Match::client(&declared);
    loads(&mut restored);
    Release::restore_into(&game, &mut restored);
    assert_eq!(restored.part::<Area>(id), game.part::<Area>(id));
    assert_eq!(restored.part::<AreaTrigger>(id), &trigger);
    // It triggers by its end, at tick 9 at the latest; a unit that is no area has no trigger.
    let entity = game.unit_entity(id);
    let triggers = |at| AreaTrigger::at(Tick::new(at)).check(&game, entity);
    assert_eq!([5, 9, 10].map(triggers), [true, true, false]);
    let id = Units::spawn(&mut game, Position::ORIGIN, Team::new(0), ());
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
    let warded = Units::spawn_of(&mut game, warding, Position::ORIGIN, Team::new(0), ());
    let area = Area::new(by, None, Tick::LIMIT);
    assert!(area.check(&game, entity));
    assert!(!area.check(&game, game.unit_entity(warded)));
}
