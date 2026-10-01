use std::collections::BTreeMap;
use std::mem;

use bevy_ecs::world::World;
use campfire_sim::{StableId, Tick};

use crate::abilities::ability_book::AbilityId;
use crate::combat::Combat;
use crate::mode::Mode;
use crate::mode::match_end::MatchEnd;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_state::ModeState;
use crate::mode::picks::Picks;
use crate::mode::player_resources::PlayerResources;
use crate::orders::Orders;
use crate::scripts::effect::Effect;
use crate::scripts::error::CallError;
use crate::scripts::hook::ScriptRole;
use crate::scripts::state_value::StateValue;
use crate::stats::Stats;
use crate::stats::modifier_book::{Applier, ModifierId};
use crate::stats::modifier_handle::ModifierHandle;
use crate::values::name_table::NameTable;
use crate::values::param::Param;
use crate::values::scalar::Scalar;

/// What the running call reads and queues, beside the units the view holds: its role, its
/// acting unit, its params, the mode's state as a mode call sees it, and the effects it
/// queued. One frame serves the whole match, its buffers cleared and filled again, so a call
/// allocates none of them.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    /// Every loaded ability's params, one run per ability, by ability id; and every loaded
    /// modifier's, by modifier id.
    params: NameTable<Param>,
    modifier_params: NameTable<Param>,
    /// The running call's role; none between calls.
    role: Option<ScriptRole>,
    /// Its acting unit: a cast's caster, a modifier's source, the unit that thinks; none for the
    /// mode.
    acting: Option<StableId>,
    /// The ability whose params it reads, at `rank`, and its modifier's.
    ability: Option<AbilityId>,
    rank: u8,
    modifier: Option<ModifierId>,
    /// The depth of the chain of combat events it runs in: 0 for a cast.
    depth: u8,
    /// Its params at its rank, in the order of their names: its ability's, and its modifier's.
    values: Vec<Scalar>,
    modifier_values: Vec<Scalar>,
    /// Whether it is a pure hook's, whose `ctx` only reads.
    pub(crate) pure: bool,
    /// The mode's state and players' choices as a mode call sees them, which it writes and reads
    /// back; whether the match ended, before the call or in it.
    pub(crate) state: Vec<StateValue>,
    pub(crate) picks: Picks,
    pub(crate) ended: bool,
    /// The players' resources as the call sees them, when the match has them, and whether the
    /// call changed them.
    resources: Option<PlayerResources>,
    resources_written: bool,
    pub(crate) effects: Vec<Effect>,
    /// The modifier handles the call took, which write back when it applies.
    pub(crate) handles: Vec<ModifierHandle>,
}

impl Frame {
    /// Adds the params of `ability`, the one the book loads next.
    pub(crate) fn add_params(&mut self, ability: AbilityId, params: &BTreeMap<String, Param>) {
        let run = self.params.push(
            params
                .iter()
                .map(|(name, param)| (name.as_str(), param.clone())),
        );
        debug_assert_eq!(run, ability.index(), "one run of params per ability");
    }

    /// Adds the params of `modifier`, the one the book loaded last.
    pub(crate) fn add_modifier_params(
        &mut self,
        modifier: ModifierId,
        params: &BTreeMap<String, Param>,
    ) {
        let run = self.modifier_params.push(
            params
                .iter()
                .map(|(name, param)| (name.as_str(), param.clone())),
        );
        debug_assert_eq!(run, modifier.index(), "one run of params per modifier");
    }

    pub(crate) const fn role(&self) -> Option<ScriptRole> {
        self.role
    }

    pub(crate) const fn acting(&self) -> Option<StableId> {
        self.acting
    }

    /// Starts a cast of `ability` at `rank` by `caster` in `world`, with its params at that
    /// rank; a param that overflows there fails the cast.
    pub(crate) fn begin_cast(
        &mut self,
        world: &World,
        ability: AbilityId,
        rank: u8,
        caster: StableId,
    ) -> Result<(), CallError> {
        self.read_resources(world);
        self.begin(
            ScriptRole::Action,
            Some(caster),
            Some(ability),
            rank,
            None,
            0,
        )
    }

    /// Starts a hook of `modifier` at chain depth `depth`, whose instance came from `source` by
    /// `ability` at `rank`: the modifier's params, then the ability's, at that rank; a param
    /// that overflows there fails the call.
    pub(crate) fn begin_hook(
        &mut self,
        world: &World,
        modifier: ModifierId,
        ability: Option<AbilityId>,
        rank: u8,
        source: Option<StableId>,
        depth: u8,
    ) -> Result<(), CallError> {
        self.read_resources(world);
        let role = ScriptRole::Modifier;
        self.begin(role, source, ability, rank, Some(modifier), depth)
    }

    /// Starts `on_think` for `unit` in `world`.
    pub(crate) fn begin_think(&mut self, world: &World, unit: StableId) {
        self.read_resources(world);
        self.begin(ScriptRole::Ai, Some(unit), None, 1, None, 0)
            .expect("a call with no params overflows none");
    }

    /// Starts a mode call, with the mode's state, choices and resources as `world` holds them,
    /// for the call to read and write; `pure` for a hook whose `ctx` only reads.
    pub(crate) fn begin_mode(&mut self, world: &World, pure: bool) {
        self.read_resources(world);
        self.begin(ScriptRole::Mode, None, None, 1, None, 0)
            .expect("a call with no params overflows none");
        self.state.clear();
        self.state
            .extend_from_slice(&world.resource::<ModeState>().0);
        self.picks.clone_from(world.resource::<Picks>());
        self.ended = world.contains_resource::<MatchEnd>();
        self.pure = pure;
    }

    /// Reads the players' resources as `world` holds them, for the call to add to; none in a
    /// match with no mode.
    fn read_resources(&mut self, world: &World) {
        self.resources_written = false;
        match (world.get_resource::<PlayerResources>(), &mut self.resources) {
            (Some(held), Some(resources)) => resources.clone_from(held),
            (held, resources) => *resources = held.cloned(),
        }
    }

    /// The players' resources, for the call to change; `None` in a match with no mode.
    pub(crate) fn resources_mut(&mut self) -> Option<&mut PlayerResources> {
        self.resources_written = true;
        self.resources.as_mut()
    }

    fn begin(
        &mut self,
        role: ScriptRole,
        acting: Option<StableId>,
        ability: Option<AbilityId>,
        rank: u8,
        modifier: Option<ModifierId>,
        depth: u8,
    ) -> Result<(), CallError> {
        self.role = Some(role);
        self.acting = acting;
        self.ability = ability;
        self.rank = rank;
        self.modifier = modifier;
        self.depth = depth;
        self.pure = false;
        self.effects.clear();
        self.handles.clear();
        let fill = |values: &mut Vec<Scalar>, params: &[Param]| {
            values.clear();
            values.reserve_exact(params.len());
            for param in params {
                values.push(param.at(rank).ok_or(CallError::ParamOverflow)?);
            }
            Ok(())
        };
        let params = ability.map_or(&[][..], |ability| self.params.values(ability.index()));
        fill(&mut self.values, params)?;
        let params = modifier.map_or(&[][..], |modifier| {
            self.modifier_params.values(modifier.index())
        });
        fill(&mut self.modifier_values, params)
    }

    /// Applies the effects the call that ran queued, in order, each by its capability, from
    /// the call's acting unit and its ability at its rank, in tick `now`; `mode` is the match's
    /// mode, which applies the mode's effects. Then what the call wrote to modifier handles
    /// applies.
    pub(crate) fn apply(&mut self, world: &mut World, mode: Option<&ModeBook>, now: Tick) {
        let (source, ability, rank, depth) = (self.acting, self.ability, self.rank, self.depth);
        let mut effects = mem::take(&mut self.effects);
        for effect in effects.drain(..) {
            match effect {
                Effect::Combat(effect) => {
                    Combat::apply_effect(world, effect, source, ability, depth);
                }
                Effect::Modifier(effect) => {
                    let applier = Applier {
                        source,
                        ability,
                        rank,
                        passive: false,
                        aura: false,
                    };
                    let param = |name: &str| self.ability_param(ability?, rank, name);
                    Stats::apply_effect(world, effect, applier, param);
                }
                Effect::Order(order) => {
                    let unit = source.expect("an order comes from the unit that thinks");
                    Orders::apply_order(world, unit, order);
                }
                Effect::Mode(effect) => {
                    let mode = mode.expect("a mode effect comes from a match with a mode");
                    Mode::apply_effect(world, mode, now, effect);
                }
            }
        }
        self.effects = effects;
        for handle in self.handles.drain(..) {
            Stats::write_handle(world, &handle);
        }
        if let (true, Some(resources)) = (self.resources_written, &self.resources) {
            world
                .resource_mut::<PlayerResources>()
                .clone_from(resources);
        }
    }

    /// Param `name` of `ability` at `rank`, if it declares one and it resolves there.
    pub(crate) fn ability_param(&self, ability: AbilityId, rank: u8, name: &str) -> Option<Scalar> {
        let at = self.params.find(ability.index(), name)?;
        self.params.values(ability.index())[at].at(rank)
    }

    /// The running call's param `name`: its modifier's, then its ability's, if either declares
    /// one.
    pub(crate) fn param(&self, name: &str) -> Option<Scalar> {
        let own = self.modifier.and_then(|modifier| {
            let at = self.modifier_params.find(modifier.index(), name)?;
            Some(self.modifier_values[at])
        });
        own.or_else(|| {
            let at = self.params.find(self.ability?.index(), name)?;
            Some(self.values[at])
        })
    }
}
