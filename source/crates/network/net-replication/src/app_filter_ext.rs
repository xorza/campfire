use bevy_app::App;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::lifecycle::{Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Query, Res};
use bevy_replicon::prelude::{AppVisibilityExt, VisibilityFilter};
use bevy_replicon::server::visibility::client_visibility::ClientVisibility;
use bevy_replicon::server::visibility::registry::FilterRegistry;

/// Adds a visibility filter that reads disabled entities for every link.
pub trait AppFilterExt {
    /// Adds the filter `F`, evaluated for each entity as `F` comes, and for each link as it gets
    /// its `ClientVisibility` and as its client component comes or goes, a disabled entity among
    /// them (11-net-stack/replication.md, A filter reads a disabled entity for every link).
    fn add_filter<F: VisibilityFilter>(&mut self) -> &mut Self;
}

impl AppFilterExt for App {
    fn add_filter<F: VisibilityFilter>(&mut self) -> &mut Self {
        self.add_visibility_filter::<F>()
            .add_observer(DisabledFiltered::for_new_client::<F>)
            .add_observer(DisabledFiltered::on_client_insert::<F>)
            .add_observer(DisabledFiltered::on_client_remove::<F>)
    }
}

/// The evaluations of a filter for a link that Replicon's own observers skip, of the disabled
/// entities alone: Replicon reads them as the filter comes, but not as a link connects or its
/// client component changes (`bevy_replicon` 0.45.2, `server/visibility.rs`).
#[derive(Debug)]
struct DisabledFiltered;

/// The disabled entities that hold the filter `F`, of those that are not links.
type DisabledHolders<'w, 's, F> =
    Query<'w, 's, (Entity, &'static F), (With<Disabled>, Without<ClientVisibility>)>;

impl DisabledFiltered {
    fn for_new_client<F: VisibilityFilter>(
        insert: On<'_, '_, Insert<ClientVisibility>>,
        registry: Res<'_, FilterRegistry>,
        mut clients: Query<'_, '_, (&mut ClientVisibility, Option<&F::ClientComponent>)>,
        entities: DisabledHolders<'_, '_, F>,
    ) {
        let Ok((mut visibility, component)) = clients.get_mut(insert.entity) else {
            return;
        };
        Self::evaluate(
            &registry,
            insert.entity,
            component,
            &mut visibility,
            &entities,
        );
    }

    fn on_client_insert<F: VisibilityFilter>(
        insert: On<'_, '_, Insert<F::ClientComponent>>,
        registry: Res<'_, FilterRegistry>,
        mut clients: Query<'_, '_, (&F::ClientComponent, &mut ClientVisibility)>,
        entities: DisabledHolders<'_, '_, F>,
    ) {
        let Ok((component, mut visibility)) = clients.get_mut(insert.entity) else {
            return;
        };
        Self::evaluate(
            &registry,
            insert.entity,
            Some(component),
            &mut visibility,
            &entities,
        );
    }

    fn on_client_remove<F: VisibilityFilter>(
        remove: On<'_, '_, Remove<F::ClientComponent>>,
        registry: Res<'_, FilterRegistry>,
        mut clients: Query<'_, '_, &mut ClientVisibility>,
        entities: DisabledHolders<'_, '_, F>,
    ) {
        // A despawned link leaves the visibility lists at Replicon's despawn collection.
        if remove.trigger().new_archetype.is_none() {
            return;
        }
        let Ok(mut visibility) = clients.get_mut(remove.entity) else {
            return;
        };
        Self::evaluate(&registry, remove.entity, None, &mut visibility, &entities);
    }

    /// Sets the bit of `F` for each of `entities` on the link `client`, of `component`.
    fn evaluate<F: VisibilityFilter>(
        registry: &FilterRegistry,
        client: Entity,
        component: Option<&F::ClientComponent>,
        visibility: &mut ClientVisibility,
        entities: &DisabledHolders<'_, '_, F>,
    ) {
        let bit = registry.get_bit::<F>().expect("the filter is added");
        for (entity, filter) in entities {
            visibility.set(entity, bit, filter.is_visible(client, component));
        }
    }
}
