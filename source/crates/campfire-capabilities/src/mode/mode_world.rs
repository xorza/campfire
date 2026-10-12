use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_sim::{IdAllocator, SimTick};

use crate::mode::calls::Calls;
use crate::mode::match_end::MatchEnd;
use crate::mode::match_mode::MatchMode;
use crate::mode::mode_state::{ModeState, ModeSync};
use crate::mode::placed_unit::PlacedPath;
use crate::mode::save_asked::SaveAsked;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::players::players_world::{PlayersWorld, PlayersWorldCrate};
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::scripts_world::ScriptsWorldCrate;
use crate::scripts::state_value::StateValue;
use crate::units::spawn_at::SpawnAt;
use crate::units::team::Team;
use crate::units::unit_state::{SyncAll, SyncNone};

/// What the mode module gives a world, for every crate.
pub trait ModeWorld {
    /// The end of the match, once it ended.
    fn match_end(&self) -> Option<MatchEnd>;

    /// The boundary the match's mode last asked for a save at, by `ctx.save()`, by the tick
    /// it comes before.
    fn save_asked(&self) -> Option<Tick>;

    /// The mode's state in the match, every field in the order of their names, as a
    /// mode call reads it.
    fn mode_state(&self) -> Vec<StateValue>;

    /// The team of player `slot` in the match; `None` before the mode installs, or for
    /// a slot the session does not have.
    fn team_of(&self, slot: PlayerSlot) -> Option<Team>;

    /// Starts the match, before its first tick: the map's placed units spawn, on their paths and
    /// walking them as they name, then `on_match_start` runs. A timer it sets counts from the
    /// start. An error when the call fails: a match its mode cannot start would run without its
    /// rules.
    fn start_mode(&mut self) -> Result<(), CallError>;
}

impl ModeWorld for World {
    fn match_end(&self) -> Option<MatchEnd> {
        self.get(self.match_entity()?).copied()
    }

    fn save_asked(&self) -> Option<Tick> {
        self.get_resource::<SaveAsked>().map(|asked| asked.at())
    }

    fn mode_state(&self) -> Vec<StateValue> {
        let mut state = Vec::new();
        let (none, all) = (
            self.mode_fields::<SyncNone>(),
            self.mode_fields::<SyncAll>(),
        );
        self.non_send::<MatchMode>()
            .schema
            .merge_state(none, all, &mut state);
        state
    }

    fn team_of(&self, slot: PlayerSlot) -> Option<Team> {
        self.get_non_send::<MatchMode>()?.teams.of(slot)
    }

    fn start_mode(&mut self) -> Result<(), CallError> {
        let book = self.non_send::<MatchMode>().clone();
        let teams = &book.teams;
        let slots = (0..teams.players()).map(PlayerSlot::new);
        self.spawn_match_entities(
            slots.map(|slot| teams.of(slot).expect("every player has a team")),
        );
        for placed in &book.placed {
            let at = SpawnAt {
                id: self.resource_mut::<IdAllocator>().allocate(),
                unit_type: placed.unit_type,
                team: placed.team,
                pos: placed.pos,
                angle: placed.angle,
            };
            match placed.path {
                None => book.spawn(self, at, None, ()),
                Some(PlacedPath { path, from: None }) => {
                    book.spawn(self, at, None, OnPath::new(path))
                }
                Some(PlacedPath {
                    path,
                    from: Some(from),
                }) => {
                    let walker = (OnPath::new(path), PathWalker::start(from, at.id));
                    book.spawn(self, at, None, walker)
                }
            };
        }
        if !book.schema.hooks.contains(Hook::OnMatchStart) {
            return Ok(());
        }
        let now = self.resource::<SimTick>().start();
        self.mode_calls(now, |call| {
            call.run(Pool::Mode, Hook::OnMatchStart, (call.ctx.clone(),))
        })
        .map_err(CallError::from_script)
    }
}

/// What the mode module gives a world, for its own crate.
pub(crate) trait ModeWorldCrate {
    /// The fields of the mode's state class of sync `S`, on the match's entity, which every
    /// match with a mode spawns with them.
    fn mode_fields<S: ModeSync>(&self) -> &[StateValue];

    /// Writes `values`, the fields of the class of sync `S` as a call left them, unless they are
    /// what the match holds, so a call that changed none marks nothing changed.
    fn commit_mode_fields<S: ModeSync>(&mut self, values: &[StateValue]);

    /// Runs `calls` in one script batch at the time `now`, and gives what `calls` gave.
    fn mode_calls<T>(&mut self, now: Tick, calls: impl FnOnce(&mut Calls<'_, '_>) -> T) -> T;
}

impl ModeWorldCrate for World {
    fn mode_fields<S: ModeSync>(&self) -> &[StateValue] {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        let state = self.get::<ModeState<S>>(entity);
        state
            .expect("the match's entity holds the mode's state")
            .values()
    }

    fn commit_mode_fields<S: ModeSync>(&mut self, values: &[StateValue]) {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        let mut state = self
            .get_mut::<ModeState<S>>(entity)
            .expect("the match's entity holds the mode's state");
        if state.values() != values {
            state.values_mut().clone_from_slice(values);
        }
    }

    fn mode_calls<T>(&mut self, now: Tick, calls: impl FnOnce(&mut Calls<'_, '_>) -> T) -> T {
        self.script_batch(|batch| {
            let ctx = batch.ctx().clone();
            calls(&mut Calls { batch, ctx, now })
        })
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {

    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::{IdAllocator, Position, StableId};

    use crate::mode::match_mode::MatchMode;
    use crate::scripts::ctx::Ctx;
    use crate::units::spawn_at::SpawnAt;
    use crate::units::spawner::Spawner;
    use crate::units::team::Team;

    /// What the mode module gives a world for the tests and benches of every crate.
    pub trait ModeWorldInternals {
        /// Spawns a unit of the mode's type `name` on `team` at `pos`, with the parts its type's
        /// kit gives, as the mode's own spawns do.
        fn spawn_typed(&mut self, name: &str, team: Team, pos: Position) -> StableId;
    }

    impl ModeWorldInternals for World {
        fn spawn_typed(&mut self, name: &str, team: Team, pos: Position) -> StableId {
            let ctx = self.non_send::<Ctx>().clone();
            let book = self.non_send::<MatchMode>().clone();
            let unit_type = ctx
                .view()
                .unit_type_named(name)
                .filter(|&unit_type| book.kit(unit_type).is_some())
                .expect("a type of the mode, with a kit");
            let id = self.resource_mut::<IdAllocator>().allocate();
            let spawner = self.non_send::<Spawner>().clone();
            spawner.spawn(
                self,
                SpawnAt {
                    id,
                    unit_type,
                    team,
                    pos,
                    angle: Num::ZERO,
                },
                None,
            );
            id
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use campfire_sim::internals::SimWorldInternals;
    use std::slice;

    use bevy_ecs::entity::Entity;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_math::Num;
    use campfire_script::rhai::Dynamic;
    use campfire_script::{Budget, ScriptHost};
    use campfire_sim::{
        EntityIndex, Position, SimTick, SimWorld, SlotEvent, SlotEventKind, StableId, TickInputs,
    };

    use crate::combat::damage::{Damage, DamageCause};
    use crate::combat::pass_queue::PassQueue;
    use crate::mode::mode_book::ModeBook;
    use crate::mode::mode_input::{InputValue, ModeInput};
    use crate::mode::mode_world::ModeWorld;
    use crate::navigation::path_walker::{PathEnd, PathWalker};
    use crate::release::release_world::test_world::ReleaseWorldTest;
    use crate::scripts::call_start::CallStart;
    use crate::scripts::ctx::Ctx;
    use crate::scripts::error::CallError;
    use crate::scripts::error::internals::FailureKind;
    use crate::scripts::script_failures::ScriptFailures;
    use crate::scripts::script_role::ScriptRole;
    use crate::scripts::state_value::StateValue;
    use crate::stats::application::{Application, NewInstance};
    use crate::stats::instance::StatShare;
    use crate::stats::level::Level;
    use crate::stats::life::Life;
    use crate::stats::modifiers::Modifiers;
    use crate::stats::stat_book::StatBook;
    use crate::stats::stat_op::StatOp;
    use crate::stats::stats_world::internals::StatsWorldInternals;
    use crate::stats::stats_world::test_world::StatsWorldTest;
    use crate::stats::unit_stats::UnitStats;
    use crate::units::action_id::ActionId;
    use crate::units::owner::Owner;
    use crate::units::tag_set::TagSet;
    use crate::units::team::Team;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::rank::Rank;
    use crate::values::stat::Stat;

    /// What the mode module gives a world for its own tests.
    pub(crate) trait ModeWorldTest {
        /// The abilities the test mode's match loads.
        fn mode_abilities(&self) -> ModeAbilities;

        /// Player `slot` picks `hero` in a tick: the hero it then owns.
        fn pick(&mut self, slot: u32, hero: &str) -> Entity;

        /// Runs a tick with `inputs`, each a player's slot and a mode input.
        fn tick_inputs(&mut self, inputs: &[(u32, ModeInput<'_>)]);

        /// Runs a tick in which the session logged `events`, each a player's join or leave.
        fn tick_slots(&mut self, events: &[(u32, SlotEventKind)]);

        /// Unit `id`.
        fn entity_by_id(&self, id: u64) -> Entity;

        /// The state fields `phase`, `seen`, `count` and `inputs`.
        fn tracked_state(&self) -> [StateValue; 4];

        /// The state field `name`: the state holds the fields in the order of their names.
        fn field(&self, name: &str) -> StateValue;

        /// Each unit: its id, where it stands, its team, and the end of a path it walks from.
        fn placed_units(&self) -> Vec<(u64, Position, usize, Option<PathEnd>)>;

        /// The kind of each of the tick's failed calls.
        fn failures(&self) -> Vec<FailureKind>;

        /// A grunt of 1000 health on `team`, whose modifier adds `stats` by name.
        fn stat_grunt(&mut self, team: u8, stats: &[(&str, Num)]) -> StableId;

        /// Queues `amount` of damage of the kind `kind` from `source` to `target`, dealt by
        /// `cause`.
        fn damage_of_kind(
            &mut self,
            source: Option<StableId>,
            target: StableId,
            amount: i64,
            kind: &str,
            cause: DamageCause,
        );

        /// Runs `probe(ctx, unit)` of `source` as a call of `role`, acting as `actor` but in the
        /// mode's, then applies its effects: what it returned, or why it failed.
        fn probe_as(
            &mut self,
            source: &str,
            role: ScriptRole,
            actor: StableId,
            id: StableId,
        ) -> Result<Dynamic, CallError>;
    }

    impl ModeWorldTest for World {
        fn mode_abilities(&self) -> ModeAbilities {
            *self.resource::<ModeAbilities>()
        }

        fn pick(&mut self, slot: u32, hero: &str) -> Entity {
            let input = ModeInput {
                name: "hero",
                value: InputValue::String(hero),
            };
            self.tick_inputs(&[(slot, input)]);
            let player = PlayerSlot::new(slot);
            let mut owned = self.query::<(Entity, &Owner)>();
            let mut heroes = owned.iter(self).filter(|(_, owner)| owner.slot() == player);
            let (hero, _) = heroes.next().expect("the player picked a hero");
            assert!(heroes.next().is_none(), "the player owns one hero");
            hero
        }

        fn tick_inputs(&mut self, inputs: &[(u32, ModeInput<'_>)]) {
            for (slot, input) in inputs {
                let payload = ModeInput::payload(slice::from_ref(input));
                self.push_input(PlayerSlot::new(*slot), &payload);
            }
            self.step();
        }

        fn tick_slots(&mut self, events: &[(u32, SlotEventKind)]) {
            for &(slot, kind) in events {
                let slot = PlayerSlot::new(slot);
                let mut inputs = self.resource_mut::<TickInputs>();
                inputs.push_slot_event(SlotEvent { slot, kind });
            }
            self.step();
        }

        fn entity_by_id(&self, id: u64) -> Entity {
            let mut units = self.resource::<EntityIndex>().iter();
            units.find(|(unit, _)| unit.get() == id).unwrap().1
        }

        fn tracked_state(&self) -> [StateValue; 4] {
            ["phase", "seen", "count", "inputs"].map(|name| self.field(name))
        }

        fn field(&self, name: &str) -> StateValue {
            let ctx = self.non_send::<Ctx>();
            let at = ModeBook::of(ctx)
                .unwrap()
                .schema
                .state_field_named(name)
                .unwrap()
                .index;
            self.mode_state()[at].clone()
        }

        fn placed_units(&self) -> Vec<(u64, Position, usize, Option<PathEnd>)> {
            self.resource::<EntityIndex>()
                .iter()
                .filter(|&(_, entity)| self.entity(entity).contains::<UnitType>())
                .map(|(id, entity)| {
                    let unit = self.entity(entity);
                    (
                        id.get(),
                        *unit.get::<Position>().unwrap(),
                        unit.get::<Team>().unwrap().index(),
                        unit.get::<PathWalker>().map(|walker| walker.walks_from()),
                    )
                })
                .collect()
        }

        fn failures(&self) -> Vec<FailureKind> {
            let failures = self.non_send::<ScriptFailures>().calls();
            failures.into_iter().map(|failure| failure.kind).collect()
        }

        fn stat_grunt(&mut self, team: u8, stats: &[(&str, Num)]) -> StableId {
            let book = self.resource::<StatBook>();
            let changes: Vec<_> = stats
                .iter()
                .map(|&(name, _)| {
                    (
                        book.named(&Stat::named(name).unwrap()).unwrap(),
                        StatOp::Add,
                    )
                })
                .collect();
            let id = self.push_modifier_changes(&changes, TagSet::default());
            let shares = stats
                .iter()
                .map(|&(_, value)| StatShare { value, live: None });
            let instance = NewInstance {
                stats: shares.collect(),
                ..NewInstance::bare(id, None)
            };
            let modifiers = Modifiers::bundle([Application::refresh(instance)]);
            let grunt = self.non_send::<View>().unit_type_named("grunt").unwrap();
            let parts = (
                Life::full(Num::int(1000)),
                Level::default(),
                UnitStats::default(),
                modifiers,
            );
            self.spawn_unit_of(grunt, Position::ORIGIN, Team::new(team), parts)
        }

        fn damage_of_kind(
            &mut self,
            source: Option<StableId>,
            target: StableId,
            amount: i64,
            kind: &str,
            cause: DamageCause,
        ) {
            let kind = self
                .non_send::<View>()
                .damage_kind_named(kind)
                .expect("the mode names the damage kind");
            self.resource_mut::<PassQueue>().push_damage(Damage {
                source,
                target,
                amount: Num::int(amount),
                kind,
                cause,
                action: None,
                depth: 0,
                hit: None,
            });
        }

        fn probe_as(
            &mut self,
            source: &str,
            role: ScriptRole,
            actor: StableId,
            id: StableId,
        ) -> Result<Dynamic, CallError> {
            let ctx = self.non_send::<Ctx>().clone();
            ctx.view().read(self);
            let now = self.resource::<SimTick>().start();
            match role {
                ScriptRole::Mode => ctx.frame().begin_mode(self, false, now),
                ScriptRole::Ai => ctx.frame().begin_think(self, actor, now),
                ScriptRole::Action => {
                    let strike = self.mode_abilities().strike;
                    let start = CallStart::cast(strike, Rank::FIRST, actor, 0, now);
                    ctx.frame().begin(self, start).unwrap();
                }
                ScriptRole::Modifier => {
                    let blessing = self.modifier_named(0, "blessing").unwrap();
                    let start = CallStart {
                        acting: Some(actor),
                        ..CallStart::hook(blessing, 0, 1, now)
                    };
                    ctx.frame().begin(self, start).unwrap();
                }
            }
            let handle = ctx.view().unit(id).unwrap();
            let returned = {
                let mut host = self.non_send_mut::<ScriptHost>();
                let script = host.compile(source).unwrap();
                let mut budget = Budget::new(u64::MAX);
                host.call(&mut budget, script, "probe", (ctx.clone(), handle))
            };
            let returned = returned.map_err(CallError::from_script)?;
            ctx.apply(self);
            Ok(returned)
        }
    }

    /// The abilities the test mode's match loads.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct ModeAbilities {
        /// The one spell's ability.
        pub(crate) blink: ActionId,
        /// Hero X's ability, of 2 ranks.
        pub(crate) strike: ActionId,
    }
}
