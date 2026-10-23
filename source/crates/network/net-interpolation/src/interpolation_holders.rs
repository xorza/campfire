//! Where each delayed interpolation rule works, as the registry resolves each interpolated
//! archetype.

use crate::interpolation_rule_id::InterpolationRuleId;
use crate::rule_resolution::RuleResolutionScratch;
use bevy_ecs::archetype::ArchetypeGeneration;
use bevy_ecs::component::ComponentId;
use bevy_ecs::prelude::*;
use bevy_platform::collections::HashMap;
use campfire_net_frame_interpolation::ArchetypeSet;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::Interpolated;

/// What no rule claims anywhere.
static NOWHERE: ArchetypeSet = ArchetypeSet::new();

/// Where each delayed interpolation rule works, on the archetypes that hold `Interpolated`: each
/// typed system reads the sets of its own rule or component. The delayed pipeline's plugin puts
/// it in, so its presence says that the app runs the pipeline.
#[derive(Resource, Debug)]
pub(crate) struct InterpolationHolders {
    pub(crate) marker_component_id: ComponentId,
    pub(crate) generation: ArchetypeGeneration,
    /// By rule: the archetypes whose apply the rule holds.
    pub(crate) apply: Vec<ArchetypeSet>,
    /// By component: the archetypes where the rule that holds its history maintains it.
    pub(crate) history: HashMap<ComponentTypeId, ArchetypeSet>,
    pub(crate) scratch: RuleResolutionScratch,
}

impl FromWorld for InterpolationHolders {
    fn from_world(world: &mut World) -> Self {
        Self {
            marker_component_id: world.register_component::<Interpolated>(),
            generation: ArchetypeGeneration::initial(),
            apply: Vec::new(),
            history: HashMap::default(),
            scratch: RuleResolutionScratch::default(),
        }
    }
}

impl InterpolationHolders {
    /// The archetypes whose apply `rule` holds.
    pub(crate) fn apply(&self, rule: InterpolationRuleId) -> &ArchetypeSet {
        self.apply.get(rule.0).unwrap_or(&NOWHERE)
    }

    /// The archetypes where `type_id`'s history is maintained.
    pub(crate) fn history(&self, type_id: ComponentTypeId) -> &ArchetypeSet {
        self.history.get(&type_id).unwrap_or(&NOWHERE)
    }
}
