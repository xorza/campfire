use bevy_ecs::component::Component;
use bevy_ecs::query::{Changed, Has, Or, QueryFilter, ReadOnlyQueryData};

/// The parts of a unit a source of the script view reads into its row, and the filter that holds
/// for a unit whose parts changed: one `Changed` for each part, so a source cannot read a part
/// whose change the view misses. A part that comes or goes moves the unit to another archetype,
/// which the view checks on its own.
pub(crate) trait RowParts: ReadOnlyQueryData + 'static {
    type Changed: QueryFilter + 'static;
}

impl<T: Component> RowParts for &'static T {
    type Changed = Changed<T>;
}

impl<T: Component> RowParts for Option<&'static T> {
    type Changed = Changed<T>;
}

impl<T: Component> RowParts for Has<T> {
    type Changed = Changed<T>;
}

impl<A: RowParts, B: RowParts> RowParts for (A, B) {
    type Changed = Or<(A::Changed, B::Changed)>;
}

impl<A: RowParts, B: RowParts, C: RowParts> RowParts for (A, B, C) {
    type Changed = Or<(A::Changed, B::Changed, C::Changed)>;
}

impl<A: RowParts, B: RowParts, C: RowParts, D: RowParts> RowParts for (A, B, C, D) {
    type Changed = Or<(A::Changed, B::Changed, C::Changed, D::Changed)>;
}

impl<A: RowParts, B: RowParts, C: RowParts, D: RowParts, E: RowParts> RowParts for (A, B, C, D, E) {
    type Changed = Or<(A::Changed, B::Changed, C::Changed, D::Changed, E::Changed)>;
}

impl<A: RowParts, B: RowParts, C: RowParts, D: RowParts, E: RowParts, F: RowParts, G: RowParts>
    RowParts for (A, B, C, D, E, F, G)
{
    type Changed = Or<(
        A::Changed,
        B::Changed,
        C::Changed,
        D::Changed,
        E::Changed,
        F::Changed,
        G::Changed,
    )>;
}
