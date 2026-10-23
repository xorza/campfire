use crate::frame_interpolation_history::FrameInterpolationHistory;
use crate::frame_registry::{self, FrameRegistry};
use crate::frame_rule::FrameRule;
use crate::frame_systems::{install_frame_history, install_visual_write};
use bevy_app::App;
use bevy_ecs::archetype::Archetype;
use bevy_ecs::prelude::*;
use bevy_ecs::query::ArchetypeFilter;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::HistoryComponent;

/// The frame rules of an app.
pub trait AppFrameInterpolationExt {
    /// Registers `rule`, which shows `C` through `V`, on the archetypes `F` matches, built as the
    /// app finishes its plugins, when each component's frame source is known.
    ///
    /// # Panics
    ///
    /// When `V` is `C`, which keeps the simulated value, when `C` is shown through another
    /// visual, or `V` shows another component, or after the app finished its plugins.
    fn register_frame_rule<C, V, F>(&mut self, rule: FrameRule<C, V>) -> &mut Self
    where
        C: HistoryComponent,
        V: HistoryComponent,
        F: ArchetypeFilter + 'static;
}

impl AppFrameInterpolationExt for App {
    fn register_frame_rule<C, V, F>(&mut self, rule: FrameRule<C, V>) -> &mut Self
    where
        C: HistoryComponent,
        V: HistoryComponent,
        F: ArchetypeFilter + 'static,
    {
        assert_ne!(
            ComponentTypeId::of::<C>(),
            ComponentTypeId::of::<V>(),
            "{} is shown through itself, which keeps the simulated value",
            core::any::type_name::<C>()
        );
        QueryState::<&Archetype, F>::new(self.world_mut());
        let live = self.world_mut().register_component::<C>();
        let history = self
            .world_mut()
            .register_component::<FrameInterpolationHistory<C>>();
        let mut registry = frame_registry::open(self);
        registry.record_visual::<C, V>(history);
        registry.defer(Box::new(move |app: &mut Self| {
            let mut registry = app.world_mut().resource_mut::<FrameRegistry>();
            let source = registry.source(&ComponentTypeId::of::<C>()).cloned();
            let installs_history = registry.install_history_once::<C>();
            if installs_history {
                install_frame_history::<C>(app);
            }
            let writes = rule.map.is_some();
            let id =
                app.world_mut()
                    .resource_scope(|world, mut registry: Mut<'_, FrameRegistry>| {
                        registry.insert_rule::<C, F>(
                            world.components(),
                            live,
                            rule.priority,
                            writes,
                        )
                    });
            if let Some(map) = rule.map {
                install_visual_write::<C, V>(app, id, map, source);
            }
        }));
        self
    }
}
