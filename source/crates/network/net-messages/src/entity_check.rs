use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryFilter;
use bevy_ecs::system::Query;

/// Tells if an entity is one that a peer may name, for
/// [`ReceiveMapView::Checked`](crate::ReceiveMapView::Checked) (11-net-stack/api.md, A server
/// accepts from a client an entity it replicates, or the link's own).
///
/// The view holds it as `dyn`: the view goes through fn pointers that the registry stores, which
/// fix its type, and this crate cannot name the filter of the app's query.
pub trait EntityCheck: Sync + core::fmt::Debug {
    fn holds(&self, entity: Entity) -> bool;
}

impl<F: QueryFilter> EntityCheck for Query<'_, '_, (), F> {
    fn holds(&self, entity: Entity) -> bool {
        self.contains(entity)
    }
}
