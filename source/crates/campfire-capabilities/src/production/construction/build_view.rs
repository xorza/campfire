use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_math::{Num, Vec3};
use campfire_sim::{EntityIndex, Position, SimTick, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::action_target::ActionTarget;
use crate::actions::kind_spec::KindSpec;
use crate::actions::purse::Purse;
use crate::actions::slot_aim::SlotAim;
use crate::geometry::metric::Metric;
use crate::geometry::shape::Shape;
use crate::navigation::destination::Destination;
use crate::navigation::route::Route;
use crate::players::player_resources::PlayerResources;
use crate::production::build_specs::{BuildSpec, BuildSpecs, Style};
use crate::production::build_target::BuildTarget;
use crate::production::builder::{BuildOrder, Builder};
use crate::production::construction::Construction;
use crate::production::construction::step::{Start, Step};
use crate::production::held::Held;
use crate::production::holdings::Holdings;
use crate::production::placement::Placement;
use crate::production::requirements::Requirements;
use crate::production::site::Site;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::pools::Pools;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::values::relation::Relation;

/// What a build order reads of the match: the books, the players' resources and modifiers, the
/// placement, the builders, the sites, and the living, complete units the players own.
#[derive(SystemParam, Debug)]
pub(crate) struct BuildView<'w, 's> {
    pub(super) tick: Res<'w, SimTick>,
    pub(super) index: Res<'w, EntityIndex>,
    pub(super) book: Res<'w, ActionBook>,
    pub(super) builds: Res<'w, BuildSpecs>,
    pub(super) requirements: Res<'w, Requirements>,
    pub(super) metric: Res<'w, Metric>,
    pub(super) resources: Option<Res<'w, PlayerResources>>,
    pub(super) modifiers: Option<Res<'w, PlayerModifiers>>,
    pub(super) placement: Placement<'w, 's>,
    pub(super) builders: Query<
        'w,
        's,
        (
            Entity,
            &'static StableId,
            &'static Position,
            Option<&'static Body>,
            &'static Team,
            Option<&'static Owner>,
            &'static ActionSlots,
            Option<&'static Pools>,
            &'static Builder,
            Option<&'static Destination>,
            Option<&'static Route>,
            Option<&'static UnitTags>,
        ),
        Without<Dead>,
    >,
    pub(super) sites: Query<
        'w,
        's,
        (
            &'static Position,
            &'static Body,
            &'static UnitType,
            Option<&'static Owner>,
            &'static Site,
        ),
        Without<Dead>,
    >,
    pub(super) owned:
        Query<'w, 's, (&'static UnitType, &'static Owner), (Without<Dead>, Without<Site>)>,
    pub(super) changed: Query<'w, 's, Entity, (Changed<Builder>, Without<Dead>)>,
}

/// A builder's build of its order's slot: the building's type, its spec, and its range.
#[derive(Debug, Clone, Copy)]
pub(super) struct Building<'a> {
    pub(super) unit_type: UnitType,
    pub(super) spec: BuildSpec<'a>,
    pub(super) range: Num,
}

/// Where a build's building stands: at the order's point, at the builder's height, its box
/// turned by the order's angle, or the site's.
#[derive(Debug, Clone, Copy)]
pub(super) struct Placed {
    pub(super) at: Position,
    pub(super) body: Body,
}

impl BuildView<'_, '_> {
    /// Fills `held` with the living, complete units the players own, by type.
    pub(super) fn held(&self, held: &mut Vec<Held>) {
        let units = self.owned.iter().map(|(&unit_type, owner)| Held {
            owner: owner.slot(),
            unit_type,
        });
        Held::collect(held, units);
    }

    /// The build in `slot` of `slots`; `None` for a slot that holds no build.
    fn building(&self, slots: &ActionSlots, slot: u8) -> Option<Building<'_>> {
        let action = slots.slot(slot)?.action?;
        let KindSpec::Build(unit_type) = self.book.get(action)?.kind else {
            return None;
        };
        let range = self.book.meters(slots, slot);
        let spec = self
            .builds
            .of(action)
            .expect("a build's spec is in the book");
        Some(Building {
            unit_type,
            spec,
            range,
        })
    }

    /// Where the build at `target` places its building, by a builder at `from` of `owner`: the
    /// point's box, or a living site of the same building and player.
    pub(super) fn placed(
        &self,
        building: Building<'_>,
        from: Position,
        owner: Option<&Owner>,
        target: BuildTarget,
    ) -> Option<Placed> {
        match target {
            BuildTarget::Point { x, z, angle } => {
                let at = Vec3::new(x, from.get().y, z);
                Some(Placed {
                    at: Position::new(at).expect("a point within the bounds"),
                    body: building.spec.form.at(angle),
                })
            }
            BuildTarget::Site(site) => {
                let (&at, &body, &unit_type, site_owner, _) =
                    self.sites.get(self.index.get(site)?).ok()?;
                let same = unit_type == building.unit_type && site_owner == owner;
                same.then_some(Placed { at, body })
            }
        }
    }

    /// The start of the build at a point of the builder of `entity`'s `order`, when it passes
    /// its action's checks, its requirements, by `held`, and its placement.
    pub(super) fn start(&self, entity: Entity, order: BuildOrder, held: &[Held]) -> Option<Start> {
        let (_, _, &from, _, &team, owner, slots, pools, ..) = self.builders.get(entity).ok()?;
        let BuildTarget::Point { angle, .. } = order.target else {
            return None;
        };
        let building = self.building(slots, order.slot)?;
        let placed = self.placed(building, from, owner, order.target)?;
        let purse = Purse::of(pools, self.resources.as_deref(), owner);
        let owner = owner.map(|owner| owner.slot());
        let aim = SlotAim {
            slot: order.slot,
            target: ActionTarget::Point(placed.at),
        };
        let no_target = |_| Relation::Friendly;
        let checked = self
            .book
            .check(self.tick.start(), slots, purse, aim, no_target, |_| None)?;
        let holdings = Holdings {
            units: held,
            modifiers: self.modifiers.as_deref(),
        };
        let met = holdings.meet(owner, self.requirements.of(checked.id));
        let room = self
            .placement
            .passes(team, placed.at, placed.body, building.spec);
        (met && room).then_some(Start {
            unit_type: building.unit_type,
            at: placed.at,
            angle,
            cost: checked.values.cost,
            cooldown: checked.values.cooldown,
            rank: checked.rank,
        })
    }

    /// Whether the order of the builder of `entity` passes as it applies: a point's build its
    /// checks, by `held`, a site's the site.
    pub(super) fn applies(&self, entity: Entity, held: &[Held]) -> bool {
        let Ok((_, _, &from, _, _, owner, slots, _, builder, ..)) = self.builders.get(entity)
        else {
            return false;
        };
        let Some(order) = builder.order() else {
            return true;
        };
        match order.target {
            BuildTarget::Point { .. } => self.start(entity, order, held).is_some(),
            BuildTarget::Site(_) => self
                .building(slots, order.slot)
                .and_then(|building| self.placed(building, from, owner, order.target))
                .is_some(),
        }
    }

    /// Whether the builder at `from` with a body of `body` and a build of `building` builds the
    /// building `placed`: it comes within its range.
    pub(super) fn in_range(
        &self,
        from: Position,
        body: Option<&Body>,
        building: Building<'_>,
        placed: Placed,
    ) -> bool {
        Construction::in_range(*self.metric, from, body, building.range, placed)
    }

    /// Whether the builder `holder` holds the site `site`: it lives, and its order is to build
    /// the site.
    pub(super) fn holds(&self, holder: StableId, site: StableId) -> bool {
        let found = self
            .index
            .get(holder)
            .and_then(|unit| self.builders.get(unit).ok());
        found.is_some_and(|(.., builder, _, _, _)| {
            builder
                .order()
                .is_some_and(|order| order.target == BuildTarget::Site(site))
        })
    }

    /// What the builder of `entity` does with its order this tick: out of range of its target's
    /// box, it walks to the box's point nearest it, unless its route arrived short of that point,
    /// which ends the order; in range, a point's build starts when it passes its checks, by
    /// `held`, and ends else; a builder of a site builds when the site takes it.
    pub(super) fn step(&self, entity: Entity, held: &[Held]) -> Step {
        let Ok((_, &id, &from, body, _, owner, slots, _, builder, destination, route, _)) =
            self.builders.get(entity)
        else {
            return Step::End;
        };
        let Some(order) = builder.order() else {
            return Step::End;
        };
        let Some(building) = self.building(slots, order.slot) else {
            return Step::End;
        };
        let Some(placed) = self.placed(building, from, owner, order.target) else {
            return Step::End;
        };
        if !self.in_range(from, body, building, placed) {
            let Shape::Box(boxed) = placed.body.shape() else {
                panic!("a building's body is a box");
            };
            let to = boxed.nearest_point(placed.at, from);
            return if Destination::gives_up(destination, route, to) {
                Step::End
            } else {
                Step::Walk(to)
            };
        }
        match order.target {
            BuildTarget::Point { .. } => self
                .start(entity, order, held)
                .map_or(Step::End, Step::Start),
            BuildTarget::Site(site) => {
                let site_entity = self.index.get(site).expect("a placed site stands");
                let (.., held) = self.sites.get(site_entity).expect("a placed site stands");
                match (building.spec.style, held.holder()) {
                    (Style::Alone, _) => Step::End,
                    (Style::Builders(_), _) => Step::Build,
                    (Style::Builder, Some(holder)) if holder == id => Step::Build,
                    (Style::Builder, Some(holder)) if self.holds(holder, site) => Step::End,
                    (Style::Builder, _) => Step::Take(site_entity),
                }
            }
        }
    }
}
