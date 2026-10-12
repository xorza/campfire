#[cfg(test)]
pub(crate) mod test_world {
    use std::collections::BTreeMap;

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_math::Num;
    use campfire_sim::{Position, SimWorld, StableId};

    use crate::actions::action_data::ActionData;
    use crate::actions::action_slots::{ActionSlot, ActionSlots};
    use crate::actions::action_target::ActionTarget;
    use crate::actions::actions_world::test_world::ActionsWorldTest;
    use crate::actions::in_progress::InProgress;
    use crate::actions::slot_kind::SlotKind;
    use crate::actions::slot_times::{SlotTime, SlotTimes, TimedSlots};
    use crate::combat::internals::Armed;
    use crate::combat::on_death::OnDeath;
    use crate::combat::recent_attackers::RecentAttackers;
    use crate::navigation::Navigation;
    use crate::release::release_world::test_world::ReleaseWorldTest;
    use crate::scripts::hook::Hook;
    use crate::scripts::script_failures::ScriptFailures;
    use crate::scripts::script_failures::internals::FailedCall;
    use crate::stats::level::Level;
    use crate::stats::life::Life;
    use crate::stats::modifier_clocks::ModifierClocks;
    use crate::stats::modifiers::Modifiers;
    use crate::stats::pool_id::PoolId;
    use crate::stats::pools::Pools;
    use crate::stats::stat_rule::StatRule;
    use crate::stats::stats_world::internals::StatsWorldInternals;
    use crate::stats::stats_world::test_world::StatsWorldTest;
    use crate::stats::unit_stats::UnitStats;
    use crate::units::action_id::ActionId;
    use crate::units::move_step::MoveStep;
    use crate::units::owner::Owner;
    use crate::units::team::Team;
    use crate::units::unit_type::UnitType;
    use crate::units::units_world::test_world::UnitsWorldTest;
    use crate::values::rank::Rank;
    use crate::values::stat::Stat;

    /// What the abilities module gives a world for its own tests.
    pub(crate) trait AbilitiesWorldTest {
        /// Gives the match a stat book of the stats the scaling params name, with no unit type.
        fn load_stats(&mut self);

        /// Loads `data` as the action `name` of package 0, of 5 ranks, with its script `source`.
        fn load(&mut self, name: &str, data: &ActionData, source: &str) -> ActionId;

        /// A unit of the match's own type and `team` with 500 health that stays when it dies, and
        /// `parts`, with no slots unless they hold some.
        fn spawn_combatant(&mut self, team: u8, pos: Position, parts: impl Bundle) -> StableId;

        /// A unit of `unit_type`, spawned as `spawn` spawns one.
        fn spawn_combatant_of(
            &mut self,
            unit_type: UnitType,
            team: u8,
            pos: Position,
            parts: impl Bundle,
        ) -> StableId;

        /// A unit of `team` armed as `combatant` that attacks `target`.
        fn attacker(
            &mut self,
            team: u8,
            pos: Position,
            combatant: Armed,
            target: StableId,
        ) -> StableId;

        /// Player 0's caster at the origin, on team 0, with `action` at `rank`, 100 mana and 20
        /// rage.
        fn caster(&mut self, action: ActionId, rank: u8) -> StableId;

        /// Gives unit `id` full pools: its 500 health, `mana` and `rage`.
        fn give_pools(&mut self, id: StableId, mana: i64, rage: i64);

        fn cast(&mut self, id: StableId, target: ActionTarget);

        /// Runs a tick in which each unit casts its first slot's ability at its target, as an order
        /// would make it.
        fn casts(&mut self, casts: &[(StableId, ActionTarget)]);

        fn pool(&self, id: StableId) -> i64;

        fn pool_of(&self, id: StableId, pool: PoolId) -> i64;

        fn slot(&self, id: StableId) -> ActionSlot;

        fn time(&self, id: StableId) -> SlotTime;

        fn casting(&self, id: StableId) -> Option<InProgress>;

        fn failed_calls(&self) -> Vec<FailedCall>;

        /// Gives `id` the modifier `name` from itself.
        fn give(&mut self, id: StableId, name: &str);

        /// Gives `id` the modifier `name` of package 0, from `source`.
        fn give_from(&mut self, id: StableId, source: StableId, name: &str);

        /// Each failed call of the tick: the unit it ran for and its hook.
        fn calls(&self) -> Vec<(StableId, Hook)>;

        /// Loads `data` as the weapon `name` of package 0, of one rank, with its effect lists.
        fn load_weapon(&mut self, name: &str, data: &ActionData) -> ActionId;

        /// A `fighter` of team 0 at the origin, of level 1, with `slots`.
        fn fighter(&mut self, fighter: UnitType, slots: TimedSlots) -> StableId;

        /// The shield of `id`'s `mark` from `source`, if it carries one.
        fn mark(&self, id: StableId, source: StableId) -> Option<Num>;

        /// The hop and the jab of a reaching match.
        fn reaching(&self) -> Reaching;

        /// Player 0's caster at the origin with `action`, walking a meter a tick.
        fn walking_caster(&mut self, action: ActionId) -> StableId;

        /// Where `id` stands along x.
        fn x_of(&self, id: StableId) -> Num;

        /// Veil and her type, and the places of the stats she reads, of Veil's match.
        fn veil_match(&self) -> VeilMatch;

        /// The attack damage, spell vamp and armor of `id`.
        fn veil_values(&self, id: StableId) -> [Num; 3];
    }

    impl AbilitiesWorldTest for World {
        fn load_stats(&mut self) {
            let rules = scaling_stats().map(|stat| (stat, StatRule::default()));
            self.load_stat_rules(&BTreeMap::from(rules));
        }

        fn load(&mut self, name: &str, data: &ActionData, source: &str) -> ActionId {
            let script = self.compile_hooked(source).unwrap();
            self.load_action(0, name, data, Some(script), 5).unwrap()
        }

        fn spawn_combatant(&mut self, team: u8, pos: Position, parts: impl Bundle) -> StableId {
            let unit_type = self.test_unit_type();
            self.spawn_combatant_of(unit_type, team, pos, parts)
        }

        fn spawn_combatant_of(
            &mut self,
            unit_type: UnitType,
            team: u8,
            pos: Position,
            parts: impl Bundle,
        ) -> StableId {
            let combat = (
                Life::full(Num::int(500)),
                OnDeath::Stay,
                RecentAttackers::default(),
            );
            let id = self.spawn_unit_of(unit_type, pos, Team::new(team), (combat, parts));
            let unit = self.unit_entity(id);
            self.entity_mut(unit)
                .insert_if_new(ActionSlots::new([]).timed());
            id
        }

        fn attacker(
            &mut self,
            team: u8,
            pos: Position,
            combatant: Armed,
            target: StableId,
        ) -> StableId {
            let combatant = combatant.bundle(self);
            let id = self.spawn_unit(pos, Team::new(team), combatant);
            let mut slots = self.part_mut::<ActionSlots>(id);
            slots.set_attack_target(Some(target));
            id
        }

        fn caster(&mut self, action: ActionId, rank: u8) -> StableId {
            let caster = self.spawn_combatant(
                0,
                Position::ORIGIN,
                (
                    Owner::new(PlayerSlot::new(0)),
                    ActionSlots::new([(action, SlotKind::new(0), Rank::new(rank))]).timed(),
                ),
            );
            self.give_pools(caster, 100, 20);
            caster
        }

        fn give_pools(&mut self, id: StableId, mana: i64, rage: i64) {
            let pools = [(MANA, mana), (RAGE, rage)];
            let pools = Pools::new(pools.map(|(pool, max)| (pool, Num::int(max)))).unwrap();
            self.insert_parts(id, (Life::full(Num::int(500)), pools));
        }

        fn cast(&mut self, id: StableId, target: ActionTarget) {
            self.casts(&[(id, target)]);
        }

        fn casts(&mut self, casts: &[(StableId, ActionTarget)]) {
            for &(id, target) in casts {
                let mut slots = self.part_mut::<ActionSlots>(id);
                slots.order(0, target);
            }
            self.step();
        }

        fn pool(&self, id: StableId) -> i64 {
            self.pool_of(id, MANA)
        }

        fn pool_of(&self, id: StableId, pool: PoolId) -> i64 {
            self.part::<Pools>(id)
                .current(pool)
                .unwrap()
                .to_int()
                .expect("a whole amount")
        }

        fn slot(&self, id: StableId) -> ActionSlot {
            self.part::<ActionSlots>(id).slot(0).unwrap()
        }

        fn time(&self, id: StableId) -> SlotTime {
            self.part::<SlotTimes>(id).slot(0).unwrap()
        }

        fn casting(&self, id: StableId) -> Option<InProgress> {
            let entity = self.unit_entity(id);
            let slots = self.get::<ActionSlots>(entity).unwrap();
            slots
                .in_progress()
                .filter(|underway| matches!(underway, InProgress::Order { .. }))
        }

        fn failed_calls(&self) -> Vec<FailedCall> {
            self.non_send::<ScriptFailures>().calls()
        }

        fn give(&mut self, id: StableId, name: &str) {
            self.give_from(id, id, name);
        }

        fn give_from(&mut self, id: StableId, source: StableId, name: &str) {
            let modifier = self.modifier_named(0, name).unwrap();
            let entity = self.unit_entity(id);
            if !self.entity(entity).contains::<Modifiers>() {
                self.insert_parts(id, Modifiers::default());
            }
            let from = Some((source, None, 1));
            self.give_modifier(id, modifier, from, false);
        }

        fn calls(&self) -> Vec<(StableId, Hook)> {
            let failures = self.failed_calls().into_iter();
            failures
                .map(|failure| (failure.id.unwrap(), failure.hook))
                .collect()
        }

        fn load_weapon(&mut self, name: &str, data: &ActionData) -> ActionId {
            let weapon = self.load_action(0, name, data, None, 1).unwrap();
            self.load_effect_lists(weapon, 0, data);
            weapon
        }

        fn fighter(&mut self, fighter: UnitType, slots: TimedSlots) -> StableId {
            let parts = (Level::new(1).unwrap(), UnitStats::default(), slots);
            self.spawn_combatant_of(fighter, 0, Position::ORIGIN, parts)
        }

        fn mark(&self, id: StableId, source: StableId) -> Option<Num> {
            let mark = self.modifier_named(0, "mark").unwrap();
            let entity = self.unit_entity(id);
            let modifiers = self.get::<Modifiers>(entity).unwrap();
            let clocks = self.get::<ModifierClocks>(entity).unwrap();
            modifiers.get(mark, Some(source))?;
            clocks.shield_of(modifiers, mark, Some(source))
        }

        fn reaching(&self) -> Reaching {
            *self.resource::<Reaching>()
        }

        fn walking_caster(&mut self, action: ActionId) -> StableId {
            let caster = self.caster(action, 1);
            self.insert_parts(caster, Navigation::walker(MoveStep::new(Num::ONE).unwrap()));
            caster
        }

        fn x_of(&self, id: StableId) -> Num {
            self.part::<Position>(id).get().x
        }

        fn veil_match(&self) -> VeilMatch {
            *self.resource::<VeilMatch>()
        }

        fn veil_values(&self, id: StableId) -> [Num; 3] {
            let stats = self.part::<UnitStats>(id).values();
            self.veil_match().places.map(|at| stats[at])
        }
    }

    /// Mana, the second of the match's pools, after the life pool.
    pub(crate) const MANA: PoolId = PoolId::new(1).unwrap();
    /// Rage, the third of the match's pools.
    pub(crate) const RAGE: PoolId = PoolId::new(2).unwrap();

    /// The stats of the scaling tests' mode, in its order: attack damage, an engine stat, first.
    pub(crate) fn scaling_stats() -> [Stat; 4] {
        let mut stats = ["attack_damage", "ability_power", "armor", "spell_vamp"]
            .map(|name| Stat::named(name).unwrap());
        stats.sort();
        stats
    }

    /// A match of units that walk and take orders, with Hop, which deals 10 to each enemy within
    /// half a meter of its point, and Jab, which deals 10 to its target.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct Reaching {
        pub(crate) hop: ActionId,
        pub(crate) jab: ActionId,
    }

    /// Veil's match: Veil, her type, and the places of attack damage, spell vamp and armor.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct VeilMatch {
        pub(crate) veil: StableId,
        pub(crate) veil_type: UnitType,
        pub(crate) places: [usize; 3],
    }
}
