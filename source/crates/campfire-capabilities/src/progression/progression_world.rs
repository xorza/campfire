#[cfg(test)]
pub(crate) mod test_world {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;

    use crate::progression::progression_column::ProgressionColumn;
    use crate::progression::track_book::TrackBook;
    use crate::progression::track_data::TrackData;
    use crate::units::view::View;
    use crate::values::declared_name::DeclaredName;

    /// What the progression module gives a world for its own tests.
    pub(crate) trait ProgressionWorldTest {
        /// Loads the mode's `tracks`, which the package load checked, into the match.
        fn load_tracks(&mut self, tracks: &BTreeMap<DeclaredName, TrackData>);
    }

    impl ProgressionWorldTest for World {
        fn load_tracks(&mut self, tracks: &BTreeMap<DeclaredName, TrackData>) {
            let book = TrackBook::new(tracks);
            ProgressionColumn::share(self.non_send::<View>(), book.clone());
            self.insert_resource(book);
        }
    }
}
