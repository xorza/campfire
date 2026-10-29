use bevy_app::App;
use bevy_ecs::archetype::Archetype;
use bevy_ecs::component::{ComponentId, Components};
use bevy_ecs::prelude::*;
use bevy_ecs::query::ArchetypeFilter;
use campfire_net_core::RegistrySeal;
use campfire_net_replication::{ComponentTable, ComponentTypeId, PendingInstalls};

use crate::frame_source::FrameSource;

/// Whether a rule's filter matches an archetype.
pub(crate) type FrameFilterFn = fn(&Components, &Archetype) -> bool;

/// The frame rules and what each fixed-tick component is shown through, which
/// `AppFrameInterpolationExt` fills and `FrameRegistryPlugin` seals as the app finishes its
/// plugins.
#[derive(Resource, Debug, Default)]
pub struct FrameRegistry {
    /// Every built rule in registration order, which breaks ties of priority.
    rules: Vec<RegisteredFrameRule>,
    /// What the registry holds of each fixed-tick component.
    components: ComponentTable<FrameComponent>,
    /// The frame histories of every component a rule shows, each once, set as it seals.
    history_component_ids: Vec<ComponentId>,
    pending: PendingInstalls,
    seal: RegistrySeal<()>,
}

/// One frame rule, as the holders resolve it against each archetype: its typed system was
/// installed as it was built.
#[derive(Debug)]
pub(crate) struct RegisteredFrameRule {
    pub(crate) component: ComponentTypeId,
    pub(crate) live_component_id: ComponentId,
    pub(crate) priority: usize,
    /// Whether it writes the visual, which a disabled rule does not.
    pub(crate) writes: bool,
    pub(crate) matches_archetype: FrameFilterFn,
}

/// The id of a built frame rule, its place among the registry's rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameRuleId(pub(crate) usize);

/// What the registry holds of one fixed-tick component.
#[derive(Debug, Default)]
struct FrameComponent {
    /// The visual component every rule of it writes, once a rule names it.
    visual: Option<ComponentTypeId>,
    history_component_id: Option<ComponentId>,
    /// Where frame interpolation reads its samples in place of its frame history, as another
    /// plugin set it.
    source: Option<FrameSource>,
    /// Whether its frame history system is installed.
    history_installed: bool,
}

/// The app's frame registry, made by its first registration, which comes before the app finishes
/// its plugins.
pub(crate) fn open(app: &mut App) -> Mut<'_, FrameRegistry> {
    let registry = app.world_mut().get_resource_or_init::<FrameRegistry>();
    registry.assert_open();
    registry
}

impl FrameRegistry {
    const NAME: &'static str = "frame registry";

    /// Queues `rule`, which the registry builds as it seals.
    pub(crate) fn defer(&mut self, rule: impl FnOnce(&mut App) + Send + Sync + 'static) {
        self.pending.push(rule);
    }

    /// Records that a rule shows `C` through `V`, whose frame history is `history_component_id`.
    ///
    /// # Panics
    ///
    /// When `C` is shown through another component, or `V` shows another: one component's value
    /// on screen is one visual, and one visual shows one component.
    pub(crate) fn record_visual<C: 'static, V: 'static>(
        &mut self,
        history_component_id: ComponentId,
    ) {
        let (component, visual) = (ComponentTypeId::of::<C>(), ComponentTypeId::of::<V>());
        assert!(
            self.components
                .iter()
                .all(|(type_id, entry)| type_id == component || entry.visual != Some(visual)),
            "{} shows two components",
            core::any::type_name::<V>()
        );
        let entry = self.component_mut(component);
        assert!(
            entry.visual.is_none_or(|shown| shown == visual),
            "{} is shown through two visual components",
            core::any::type_name::<C>()
        );
        entry.visual = Some(visual);
        entry.history_component_id = Some(history_component_id);
    }

    /// Has frame interpolation read `C`'s samples through `source` on the archetypes that hold
    /// its component, in place of a frame history of `C`.
    ///
    /// # Panics
    ///
    /// After the app finished its plugins, which sealed the registry.
    pub fn set_frame_source<C: 'static>(&mut self, source: FrameSource) {
        self.assert_open();
        self.component_mut(ComponentTypeId::of::<C>()).source = Some(source);
    }

    /// Where frame interpolation reads `type_id`'s samples in place of its frame history.
    pub(crate) fn source(&self, type_id: &ComponentTypeId) -> Option<&FrameSource> {
        self.component(type_id)?.source.as_ref()
    }

    /// Whether a frame rule shows `C` through `V`, as a correction of `V`'s jumps needs: only
    /// where the frame writes `V` does a correction's offset leave each frame with it.
    pub fn maps<C: 'static, V: 'static>(&self) -> bool {
        self.component(&ComponentTypeId::of::<C>())
            .is_some_and(|entry| entry.visual == Some(ComponentTypeId::of::<V>()))
    }

    /// The `FrameInterpolationHistory` component ids of every component a frame rule shows, set
    /// as the registry seals.
    ///
    /// Removing these from an entity seeds its frame interpolation again: the history update
    /// makes them again from the live value on its next run. A timeline switch removes them as it
    /// adds `FrameInterpolate` again after an era without it, during which the histories froze at
    /// a stale value that the next frames would blend from.
    pub fn frame_history_component_ids(&self) -> &[ComponentId] {
        &self.history_component_ids
    }

    /// Whether `C`'s frame history system is still to install, which it then is.
    pub(crate) fn install_history_once<C: 'static>(&mut self) -> bool {
        let entry = self.component_mut(ComponentTypeId::of::<C>());
        !core::mem::replace(&mut entry.history_installed, true)
    }

    /// Adds the rule of `C` that `writes`, on the archetypes `F` matches that hold `C`.
    pub(crate) fn insert_rule<C: 'static, F: ArchetypeFilter + 'static>(
        &mut self,
        components: &Components,
        live_component_id: ComponentId,
        priority: usize,
        writes: bool,
    ) -> FrameRuleId {
        self.assert_open();
        assert!(
            F::get_state(components).is_some(),
            "a frame rule's filter is initialized as it registers"
        );
        let id = FrameRuleId(self.rules.len());
        self.rules.push(RegisteredFrameRule {
            component: ComponentTypeId::of::<C>(),
            live_component_id,
            priority,
            writes,
            matches_archetype: matches_filter::<F>,
        });
        id
    }

    pub(crate) fn rules(&self) -> &[RegisteredFrameRule] {
        &self.rules
    }

    /// The rules registered since the last take, to build as the registry seals.
    pub(crate) fn take_pending(&mut self) -> PendingInstalls {
        self.assert_open();
        self.pending.take()
    }

    pub(crate) fn seal(&mut self) {
        self.history_component_ids = self
            .components
            .iter()
            .filter_map(|(_, entry)| entry.history_component_id)
            .collect();
        self.seal.seal(Self::NAME, ());
    }

    fn component(&self, type_id: &ComponentTypeId) -> Option<&FrameComponent> {
        self.components.get(*type_id)
    }

    /// The entry of `type_id`, which goes in if it is not there.
    fn component_mut(&mut self, type_id: ComponentTypeId) -> &mut FrameComponent {
        self.components.entry(type_id, FrameComponent::default)
    }

    fn assert_open(&self) {
        self.seal.assert_open(Self::NAME);
    }
}

fn matches_filter<F>(components: &Components, archetype: &Archetype) -> bool
where
    F: ArchetypeFilter + 'static,
{
    F::get_state(components)
        .is_some_and(|state| F::matches_component_set(&state, &|id| archetype.contains(id)))
}

#[cfg(test)]
mod tests;
