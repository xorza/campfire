use crate::archetype_set::ArchetypeSet;
use crate::frame_interpolate::FrameInterpolate;
use crate::interpolation_rule_id::InterpolationRuleId;
use crate::rule_resolution::{RuleResolutionScratch, RuleTarget};
use bevy_ecs::archetype::ArchetypeGeneration;
use bevy_ecs::component::ComponentId;
use bevy_ecs::prelude::*;
use bevy_platform::collections::HashMap;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::Interpolated;

/// What no rule claims anywhere.
static NOWHERE: ArchetypeSet = ArchetypeSet::new();

/// Where each interpolation rule works, as the registry resolves each archetype, for delayed
/// and for frame interpolation: each typed system reads the sets of its own rule or component.
#[derive(Resource, Debug)]
pub(crate) struct InterpolationHolders {
    pub(crate) delayed: Holders,
    pub(crate) frame: Holders,
}

/// The holders of one pipeline's archetypes, those that hold its marker.
#[derive(Debug)]
pub(crate) struct Holders {
    pub(crate) target: RuleTarget,
    pub(crate) marker_component_id: ComponentId,
    pub(crate) generation: ArchetypeGeneration,
    /// By rule: the archetypes whose apply the rule holds.
    pub(crate) apply: Vec<ArchetypeSet>,
    /// By component: the archetypes where the rule that holds its history maintains it.
    pub(crate) history: HashMap<ComponentTypeId, ArchetypeSet>,
    /// By component: the archetypes that hold its frame source, which its samples come from.
    pub(crate) sourced: HashMap<ComponentTypeId, ArchetypeSet>,
    pub(crate) scratch: RuleResolutionScratch,
}

impl FromWorld for InterpolationHolders {
    fn from_world(world: &mut World) -> Self {
        Self {
            delayed: Holders::new(
                RuleTarget::Delayed,
                world.register_component::<Interpolated>(),
            ),
            frame: Holders::new(
                RuleTarget::Frame,
                world.register_component::<FrameInterpolate>(),
            ),
        }
    }
}

impl Holders {
    fn new(target: RuleTarget, marker_component_id: ComponentId) -> Self {
        Self {
            target,
            marker_component_id,
            generation: ArchetypeGeneration::initial(),
            apply: Vec::new(),
            history: HashMap::default(),
            sourced: HashMap::default(),
            scratch: RuleResolutionScratch::default(),
        }
    }

    /// The archetypes whose apply `rule` holds.
    pub(crate) fn apply(&self, rule: InterpolationRuleId) -> &ArchetypeSet {
        self.apply.get(rule.0).unwrap_or(&NOWHERE)
    }

    /// The archetypes where `type_id`'s history is maintained.
    pub(crate) fn history(&self, type_id: ComponentTypeId) -> &ArchetypeSet {
        self.history.get(&type_id).unwrap_or(&NOWHERE)
    }

    /// The archetypes that read `type_id`'s samples from its frame source.
    pub(crate) fn sourced(&self, type_id: ComponentTypeId) -> &ArchetypeSet {
        self.sourced.get(&type_id).unwrap_or(&NOWHERE)
    }
}
