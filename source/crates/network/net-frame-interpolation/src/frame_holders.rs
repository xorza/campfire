use bevy_ecs::archetype::{ArchetypeGeneration, ArchetypeId, Archetypes};
use bevy_ecs::component::{ComponentId, Components};
use bevy_ecs::prelude::*;
use bevy_platform::collections::HashMap;
use campfire_net_replication::ComponentTypeId;

use crate::archetype_set::ArchetypeSet;
use crate::frame_interpolate::FrameInterpolate;
use crate::frame_registry::{FrameRegistry, FrameRuleId};

/// What no rule claims anywhere.
static NOWHERE: ArchetypeSet = ArchetypeSet::new();

/// Where each frame rule writes, as the registry's rules resolve each archetype: of the rules of a
/// component that match an archetype holding it, the one of the highest priority, the first
/// registered among equals, claims it there; it writes the visual unless it is disabled, and on a
/// frame-interpolated archetype it keeps the component's frame history, or reads its frame source.
#[derive(Resource, Debug)]
pub struct FrameHolders {
    frame_interpolate: ComponentId,
    generation: ArchetypeGeneration,
    /// By rule: the archetypes whose visual the rule writes.
    writes: Vec<ArchetypeSet>,
    /// By component: the archetypes whose visual of it a rule writes.
    shown: HashMap<ComponentTypeId, ArchetypeSet>,
    /// By component: the frame-interpolated archetypes whose frame history of it is kept.
    history: HashMap<ComponentTypeId, ArchetypeSet>,
    /// By component: the frame-interpolated archetypes that read its samples from its source.
    sourced: HashMap<ComponentTypeId, ArchetypeSet>,
    /// The winning rule of each component, kept between archetypes.
    winners: Vec<(ComponentTypeId, FrameRuleId)>,
}

impl FromWorld for FrameHolders {
    fn from_world(world: &mut World) -> Self {
        Self {
            frame_interpolate: world.register_component::<FrameInterpolate>(),
            generation: ArchetypeGeneration::initial(),
            writes: Vec::new(),
            shown: HashMap::default(),
            history: HashMap::default(),
            sourced: HashMap::default(),
            winners: Vec::new(),
        }
    }
}

impl FrameHolders {
    /// Whether a frame rule writes `C`'s visual on `archetype`: where a correction of the visual
    /// takes its offset away again each frame.
    pub fn shows<C: 'static>(&self, archetype: ArchetypeId) -> bool {
        self.shown
            .get(&ComponentTypeId::of::<C>())
            .is_some_and(|set| set.contains(archetype))
    }

    /// The archetypes whose visual `rule` writes.
    pub(crate) fn writes(&self, rule: FrameRuleId) -> &ArchetypeSet {
        self.writes.get(rule.0).unwrap_or(&NOWHERE)
    }

    /// The archetypes whose frame history of `type_id` is kept.
    pub(crate) fn history(&self, type_id: ComponentTypeId) -> &ArchetypeSet {
        self.history.get(&type_id).unwrap_or(&NOWHERE)
    }

    /// The archetypes that read `type_id`'s samples from its frame source.
    pub(crate) fn sourced(&self, type_id: ComponentTypeId) -> &ArchetypeSet {
        self.sourced.get(&type_id).unwrap_or(&NOWHERE)
    }

    /// Resolves the archetypes made since the last refresh: each frame's phases run it before
    /// their typed systems, and a system that reads [`Self::shows`] outside them runs it first.
    pub fn refresh(
        mut holders: ResMut<'_, Self>,
        archetypes: &Archetypes,
        components: &Components,
        registry: Res<'_, FrameRegistry>,
    ) {
        let holders = &mut *holders;
        let old = core::mem::replace(&mut holders.generation, archetypes.generation());
        for archetype in &archetypes[old..] {
            holders.winners.clear();
            for (index, rule) in registry.rules().iter().enumerate() {
                if !archetype.contains(rule.live_component_id)
                    || !(rule.matches_archetype)(components, archetype)
                {
                    continue;
                }
                let id = FrameRuleId(index);
                match holders
                    .winners
                    .iter_mut()
                    .find(|(component, _)| *component == rule.component)
                {
                    Some((_, winner)) if registry.rules()[winner.0].priority < rule.priority => {
                        *winner = id;
                    }
                    Some(_) => {}
                    None => holders.winners.push((rule.component, id)),
                }
            }
            let id = archetype.id();
            let framed = archetype.contains(holders.frame_interpolate);
            for &(component, rule) in &holders.winners {
                if !registry.rules()[rule.0].writes {
                    continue;
                }
                if holders.writes.len() <= rule.0 {
                    holders.writes.resize(rule.0 + 1, ArchetypeSet::new());
                }
                holders.writes[rule.0].insert(id);
                holders.shown.entry(component).or_default().insert(id);
                if !framed {
                    continue;
                }
                let sourced = registry
                    .source(&component)
                    .is_some_and(|source| archetype.contains(source.component_id));
                let lane = if sourced {
                    &mut holders.sourced
                } else {
                    &mut holders.history
                };
                lane.entry(component).or_default().insert(id);
            }
        }
    }
}
