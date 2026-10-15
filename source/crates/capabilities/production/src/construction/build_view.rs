use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_actions::{
    ActionBook, ActionCheck, ActionSlots, ActionTarget, KindSpec, Purse, SlotAim, SlotTimes,
};
use campfire_geometry::Shape;
use campfire_math::{Num, Vec3};
use campfire_players::{PlayerResources, PlayerRows};
use campfire_sim::{EntityIndex, Position, SimTick, StableId};
use campfire_stats::{PlayerModifiers, PoolBook, UnitPoolsReadOnly};
use campfire_units::{ArrivedShort, Body, Dead, Destination, MapBook, Owner};
use campfire_values::{Relation, Team, UnitTags, UnitType};
use campfire_vision::TeamSight;

use crate::build_book::{BuildBook, BuildSpec, Style};
use crate::build_target::BuildTarget;
use crate::builder::{BuildOrder, Builder};
use crate::construction::Construction;
use crate::construction::step::{Start, Step};
use crate::held::Held;
use crate::holdings::Holdings;
use crate::placement::Placement;
use crate::requirement_book::RequirementBook;
use crate::site::Site;

/// What a build order reads of the match: the books, the players' resources and modifiers, the
/// life pool, the placement, the builders, the sites, and the living, complete units the players
/// own.
#[derive(SystemParam, Debug)]
pub(crate) struct BuildView<'w, 's, S: TeamSight> {
    pub(super) tick: Res<'w, SimTick>,
    pub(super) index: Res<'w, EntityIndex>,
    pub(super) book: Res<'w, ActionBook>,
    pub(super) builds: Res<'w, BuildBook>,
    pub(super) requirements: Res<'w, RequirementBook>,
    pub(super) map: Res<'w, MapBook>,
    pub(super) resources: PlayerRows<'w, 's, PlayerResources>,
    pub(super) life: Option<Res<'w, PoolBook>>,
    pub modifiers: PlayerRows<'w, 's, PlayerModifiers>,
    pub(super) placement: Placement<'w, 's, S>,
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
            &'static SlotTimes,
            UnitPoolsReadOnly,
            &'static Builder,
            Option<&'static Destination>,
            Option<&'static ArrivedShort>,
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
pub(super) struct BuildPlace {
    pub pos: Vec3,
    pub(super) body: Body,
}

impl<S: TeamSight> BuildView<'_, '_, S> {
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
        let range = slots.meters_in(&self.book, slot);
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
        from: Vec3,
        owner: Option<&Owner>,
        target: BuildTarget,
    ) -> Option<BuildPlace> {
        match target {
            BuildTarget::Point { x, z, angle } => {
                let at = Vec3::new(x, from.y, z);
                Some(BuildPlace {
                    pos: at,
                    body: building.spec.form.at(angle),
                })
            }
            BuildTarget::Site(site) => {
                let (&at, &body, &unit_type, site_owner, _) =
                    self.sites.get(self.index.get(site)?).ok()?;
                let same = unit_type == building.unit_type && site_owner == owner;
                same.then_some(BuildPlace {
                    pos: at.get(),
                    body,
                })
            }
        }
    }

    /// The start of the build at a point of the builder of `entity`'s `order`, when it passes
    /// its action's checks, its requirements, by `held`, and its placement.
    pub(super) fn start(&self, entity: Entity, order: BuildOrder, held: &[Held]) -> Option<Start> {
        let (_, _, &from, _, &team, owner, slots, times, pools, ..) =
            self.builders.get(entity).ok()?;
        let BuildTarget::Point { angle, .. } = order.target else {
            return None;
        };
        let building = self.building(slots, order.slot)?;
        let placed = self.placed(building, from.get(), owner, order.target)?;
        let life = self.life.as_deref().and_then(PoolBook::life);
        let purse = Purse::of(pools.of(life), self.resources.of(owner));
        let modifiers = self.modifiers.of(owner);
        let owner = owner.map(|owner| owner.slot());
        let aim = SlotAim {
            slot: order.slot,
            target: ActionTarget::Point(placed.pos),
        };
        let no_target = |_| Relation::Friendly;
        let checked = times.checked_action(ActionCheck {
            book: &self.book,
            now: self.tick.start(),
            slots,
            purse,
            aim,
            relation: no_target,
            living: |_| None,
        })?;
        let holdings = Holdings { units: held };
        let met = holdings.meet(owner, modifiers, self.requirements.of(checked.id));
        let room = self
            .placement
            .passes(team, placed.pos, placed.body, building.spec);
        (met && room).then_some(Start {
            unit_type: building.unit_type,
            pos: placed.pos,
            angle,
            cost: checked.values.cost,
            cooldown: checked.values.cooldown,
            rank: checked.rank,
        })
    }

    /// Whether the order of the builder of `entity` passes as it applies: a point's build its
    /// checks, by `held`, a site's the site.
    pub(super) fn applies(&self, entity: Entity, held: &[Held]) -> bool {
        let Ok((_, _, &from, _, _, owner, slots, _, _, builder, ..)) = self.builders.get(entity)
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
                .and_then(|building| self.placed(building, from.get(), owner, order.target))
                .is_some(),
        }
    }

    /// Whether the builder at `from` with a body of `body` and a build of `building` builds the
    /// building `placed`: it comes within its range.
    pub(super) fn in_range(
        &self,
        from: Vec3,
        body: Option<&Body>,
        building: Building<'_>,
        placed: BuildPlace,
    ) -> bool {
        Construction::in_range(self.map.metric, from, body, building.range, placed)
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
    /// box, it walks to the box's point nearest it, unless it arrived short of that point,
    /// which ends the order; in range, a point's build starts when it passes its checks, by
    /// `held`, and ends else; a builder of a site builds when the site takes it.
    pub(crate) fn step(&self, entity: Entity, held: &[Held]) -> Step {
        let Ok((_, &id, &from, body, _, owner, slots, _, _, builder, destination, arrived, _)) =
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
        let Some(placed) = self.placed(building, from.get(), owner, order.target) else {
            return Step::End;
        };
        if !self.in_range(from.get(), body, building, placed) {
            let Shape::Box(boxed) = placed.body.shape() else {
                panic!("a building's body is a box");
            };
            let to = boxed.nearest_point(placed.pos, from.get());
            return if Destination::gives_up(destination, arrived, to) {
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
