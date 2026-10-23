use std::any::{TypeId, type_name};

use bevy_ecs::component::Component;
use bevy_ecs::query::{Changed, Has, Or, QueryFilter, ReadOnlyQueryData};

/// The components of a unit a source of the script view reads into its row, and the filter that
/// holds for a unit whose components changed: one `Changed` for each, so a source cannot read a
/// component whose change the view misses. A component that comes or goes moves the unit to
/// another archetype, which the view checks on its own.
pub trait RowReads: ReadOnlyQueryData + 'static {
    type Changed: QueryFilter + 'static;

    /// Each component it reads, by type and name: for the declaration audit, and for the view,
    /// which observes each one's removal, as no change tick shows it.
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

/// Implements `RowReads` for a tuple of reads: changed when any of its reads is, and reading the
/// parts each of them reads.
macro_rules! tuple_row_reads {
    ($($read:ident),+) => {
        impl<$($read: RowReads),+> RowReads for ($($read,)+) {
            type Changed = Or<($($read::Changed,)+)>;

            fn components(into: &mut Vec<(TypeId, &'static str)>) {
                $($read::components(into);)+
            }
        }
    };
}

tuple_row_reads!(A, B);
tuple_row_reads!(A, B, C);
tuple_row_reads!(A, B, C, D);
tuple_row_reads!(A, B, C, D, E);
tuple_row_reads!(A, B, C, D, E, F);
tuple_row_reads!(A, B, C, D, E, F, G);
