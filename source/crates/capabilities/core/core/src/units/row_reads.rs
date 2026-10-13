use std::any::{TypeId, type_name};

use bevy_ecs::component::Component;
use bevy_ecs::query::{Changed, Has, Or, QueryFilter, ReadOnlyQueryData};

/// The components of a unit a source of the script view reads into its row, and the filter that
/// holds for a unit whose components changed: one `Changed` for each, so a source cannot read a
/// component whose change the view misses. A component that comes or goes moves the unit to
/// another archetype, which the view checks on its own.
pub trait RowReads: ReadOnlyQueryData + 'static {
    type Changed: QueryFilter + 'static;

    /// Each component it reads, by type and name, for the declaration audit.
    fn components(into: &mut Vec<(TypeId, &'static str)>);
}

impl<T: Component> RowReads for &'static T {
    type Changed = Changed<T>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        into.push((TypeId::of::<T>(), type_name::<T>()));
    }
}

impl<T: Component> RowReads for Option<&'static T> {
    type Changed = Changed<T>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        into.push((TypeId::of::<T>(), type_name::<T>()));
    }
}

impl<T: Component> RowReads for Has<T> {
    type Changed = Changed<T>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        into.push((TypeId::of::<T>(), type_name::<T>()));
    }
}

impl<A: RowReads, B: RowReads> RowReads for (A, B) {
    type Changed = Or<(A::Changed, B::Changed)>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        A::components(into);
        B::components(into);
    }
}

impl<A: RowReads, B: RowReads, C: RowReads> RowReads for (A, B, C) {
    type Changed = Or<(A::Changed, B::Changed, C::Changed)>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        A::components(into);
        B::components(into);
        C::components(into);
    }
}

impl<A: RowReads, B: RowReads, C: RowReads, D: RowReads> RowReads for (A, B, C, D) {
    type Changed = Or<(A::Changed, B::Changed, C::Changed, D::Changed)>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        A::components(into);
        B::components(into);
        C::components(into);
        D::components(into);
    }
}

impl<A: RowReads, B: RowReads, C: RowReads, D: RowReads, E: RowReads> RowReads for (A, B, C, D, E) {
    type Changed = Or<(A::Changed, B::Changed, C::Changed, D::Changed, E::Changed)>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        A::components(into);
        B::components(into);
        C::components(into);
        D::components(into);
        E::components(into);
    }
}

impl<A: RowReads, B: RowReads, C: RowReads, D: RowReads, E: RowReads, F: RowReads> RowReads
    for (A, B, C, D, E, F)
{
    type Changed = Or<(
        A::Changed,
        B::Changed,
        C::Changed,
        D::Changed,
        E::Changed,
        F::Changed,
    )>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        A::components(into);
        B::components(into);
        C::components(into);
        D::components(into);
        E::components(into);
        F::components(into);
    }
}

impl<A: RowReads, B: RowReads, C: RowReads, D: RowReads, E: RowReads, F: RowReads, G: RowReads>
    RowReads for (A, B, C, D, E, F, G)
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

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        A::components(into);
        B::components(into);
        C::components(into);
        D::components(into);
        E::components(into);
        F::components(into);
        G::components(into);
    }
}
