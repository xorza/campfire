use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Without};
use bevy_ecs::system::{Query, Res, ResMut, SystemParam};
use campfire_actions::{
    ActionBook, ActionCheck, ActionSlots, ActionTarget, Caster, Purse, SlotAim, SlotTimes,
};
use campfire_geometry::{Metric, Placed};
use campfire_math::{Num, Vec3};
use campfire_players::{PlayerResources, PlayerRows};
use campfire_sim::{EntityIndex, Position, SimTick, StableId};
use campfire_stats::{PlayerModifiers, PoolBook, UnitPoolsReadOnly};
use campfire_units::{
    ArrivedShort, Body, BuildTarget, Dead, Destination, GroundPoint, MapBook, Owner,
};
use campfire_values::{Motion, Relation, Team, UnitTags, UnitType};
use campfire_vision::TeamSight;

use crate::build_book::{BuildBook, Building, Style};
use crate::build_start::BuildStart;
use crate::builder::{BuildOrder, Builder};
use crate::construction::step::{StartingBuild, Step};
use crate::holdings::{Holdings, OwnedUnits};
use crate::placement::Placement;
use crate::requirement_book::RequirementBook;
use crate::site::Site;

/// What a build order reads of the match: the books, the players' resources and modifiers, the
/// life pool, the placement, the builders, the sites, and the units the players own, which the
/// holdings read.
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
    owned: OwnedUnits<'w, 's>,
    holdings: ResMut<'w, Holdings>,
    pub(super) changed: Query<'w, 's, Entity, (Changed<Builder>, Without<Dead>)>,
}

/// A builder as its build's footprint reads it: where it stands, and its player.
#[derive(Debug, Clone, Copy)]
pub(super) struct BuilderAt<'a> {
    from: Vec3,
    owner: Option<&'a Owner>,
}

/// Where a build's building stands: at the order's point, at the builder's height, its box
/// turned by the order's angle, or the site's.
#[derive(Debug, Clone, Copy)]
pub(super) struct BuildFootprint {
    pub position: Vec3,
    pub(super) body: Body,
}

impl BuildFootprint {
    /// Whether `builder`, with a build's `range`, reaches the building's box, in `metric`: from
    /// the edge of the one body to the edge of the other.
    pub(super) fn reached_by(self, metric: Metric, builder: Placed, range: Num) -> bool {
        metric.reaches(
            builder,
            range,
            Placed::new(self.position, self.body.shape()),
        )
    }
}

impl<S: TeamSight> BuildView<'_, '_, S> {
    /// Follows the units, for the requirements the builds meet.
    pub(super) fn follow_holdings(&mut self) {
        self.holdings.follow(&self.owned);
    }

    /// Where the build at `target` places its building, by the builder `by`: the point's box,
    /// or a living site of the same building and player.
    pub(super) fn placed(
        &self,
        building: Building<'_>,
        by: BuilderAt<'_>,
        target: BuildTarget,
    ) -> Option<BuildFootprint> {
        match target {
            BuildTarget::Point {
                at: GroundPoint { x, z },
                angle,
            } => {
                let at = Vec3::new(x, by.from.y, z);
                Some(BuildFootprint {
                    position: at,
                    body: building.spec.form.at(angle),
                })
            }
            BuildTarget::Site(site) => {
                let (&at, &body, &unit_type, site_owner, _) =
                    self.sites.get(self.index.get(site)?).ok()?;
                let same = unit_type == building.spec.unit_type && site_owner == by.owner;
                same.then_some(BuildFootprint {
                    position: at.get(),
                    body,
                })
            }
        }
    }

    /// The start of the build at a point of the builder of `entity`'s `order`, when it passes
    /// its action's checks, its requirements, by the holdings, and its placement.
    pub(super) fn start(&mut self, entity: Entity, order: BuildOrder) -> Option<StartingBuild> {
        let (_, _, &from, _, &team, owner, slots, times, pools, .., tags) =
            self.builders.get(entity).ok()?;
        let BuildTarget::Point { angle, .. } = order.target else {
            return None;
        };
        let building = self.builds.building(&self.book, slots, order.slot)?;
        let by = BuilderAt {
            from: from.get(),
            owner,
        };
        let placed = self.placed(building, by, order.target)?;
        let life = self.life.as_deref().and_then(PoolBook::life);
        let purse = Purse::of(pools.of(life), self.resources.of(owner));
        let modifiers = self.modifiers.of(owner);
        let owner = owner.map(|owner| owner.seat());
        let aim = SlotAim {
            slot: order.slot,
            target: ActionTarget::Point(placed.position),
        };
        let no_target = |_| Relation::Friendly;
        let check = ActionCheck {
            book: &self.book,
            now: self.tick.start(),
            caster: Caster::of(tags, Motion::Own, None),
            slots,
            purse,
            aim,
            relation: no_target,
            living: |_| None,
        };
        let checked = times.start(check, None)?.checked;
        let met = self
            .holdings
            .meet(owner, modifiers, self.requirements.of(checked.id));
        let room = self
            .placement
            .passes(team, placed.position, placed.body, building.spec);
        (met && room).then_some(StartingBuild {
            unit_type: building.spec.unit_type,
            position: placed.position,
            angle,
            cost: checked.values.cost,
            cooldown: checked.values.cooldown,
            charges: checked.values.charges,
            rank: checked.rank,
        })
    }

    /// Whether the order of the builder of `entity` passes as it applies: a point's build its
    /// checks, by the holdings, a site's the site.
    pub(super) fn applies(&mut self, entity: Entity) -> bool {
        let Ok((_, _, &from, _, _, owner, slots, _, _, builder, ..)) = self.builders.get(entity)
        else {
            return false;
        };
        let Some(order) = builder.order() else {
            return true;
        };
        match order.target {
            BuildTarget::Point { .. } => self.start(entity, order).is_some(),
            BuildTarget::Site(_) => {
                let by = BuilderAt {
                    from: from.get(),
                    owner,
                };
                self.builds
                    .building(&self.book, slots, order.slot)
                    .and_then(|building| self.placed(building, by, order.target))
                    .is_some()
            }
        }
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

    /// What the builder of `entity` does with its order this tick: a point's build that starts
    /// as it is ordered starts, wherever its builder stands, when it passes its checks, by the
    /// holdings, and ends else; out of range of its target's box, the builder walks to the box's
    /// point nearest it, unless it arrived short of that point, which ends the order; in range,
    /// a point's build starts when it passes its checks, and ends else; a builder of a site
    /// builds when the site takes it.
    pub(crate) fn step(&mut self, entity: Entity) -> Step {
        let Ok((_, &id, &from, body, _, owner, slots, _, _, builder, destination, arrived, _)) =
            self.builders.get(entity)
        else {
            return Step::End;
        };
        let Some(order) = builder.order() else {
            return Step::End;
        };
        let Some(building) = self.builds.building(&self.book, slots, order.slot) else {
            return Step::End;
        };
        // An order build starts as its order applies, wherever its builder stands, and its
        // builder then walks to the site.
        if let (BuildTarget::Point { .. }, BuildStart::Order) = (order.target, building.spec.start)
        {
            return self.start(entity, order).map_or(Step::End, Step::Start);
        }
        let by = BuilderAt {
            from: from.get(),
            owner,
        };
        let Some(placed) = self.placed(building, by, order.target) else {
            return Step::End;
        };
        let builder = Placed::new(from.get(), Body::shape_of(body));
        if !placed.reached_by(self.map.metric(), builder, building.range) {
            let to = placed
                .body
                .shape()
                .nearest_point(placed.position, from.get());
            return if Destination::gives_up(destination, arrived, to) {
                Step::End
            } else {
                Step::Walk(to)
            };
        }
        match order.target {
            BuildTarget::Point { .. } => self.start(entity, order).map_or(Step::End, Step::Start),
            BuildTarget::Site(site) => {
                let site_entity = self.index.get(site).expect("a placed site stands");
                let (.., site_part) = self.sites.get(site_entity).expect("a placed site stands");
                match (building.spec.style, site_part.holder()) {
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
