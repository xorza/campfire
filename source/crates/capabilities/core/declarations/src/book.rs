use bevy_ecs::change_detection::Mut;
use bevy_ecs::component::Immutable;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_script::MatchNames;

use crate::role::Role;

/// Package data a module reads, which its builder makes from the packages before the match is
/// built, and which the build puts in as the module that declares it says, before that module
/// installs, as the install knows the module. It is an immutable resource, so no system or test
/// writes it in place, and an install's read of it holds for the match
/// (02-engine-core.md, A book is an immutable resource).
pub trait Book: Resource<Mutability = Immutable> + Sized {
    /// Whether only a match that runs scripts reads it, so a client holds none.
    const SCRIPTS_ONLY: bool;

    /// Gives the match's names the tables it holds, which the script view and the match's checks
    /// read; a call extension or a view column takes its book from the match as it begins.
    fn share(&self, names: &mut MatchNames);

    /// Whether a match of `role`, which installs its module, holds it: one that runs scripts, when
    /// only scripts read it.
    fn is_held(role: &Role) -> bool {
        role.scripts() || !Self::SCRIPTS_ONLY
    }

    /// Gives the match's names what `world`'s book holds, as it stands.
    fn share_in(world: &mut World) {
        world.resource_scope(|world, mut names: Mut<'_, MatchNames>| {
            world.resource::<Self>().share(&mut names);
        });
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::any::type_name;

    use bevy_ecs::schedule::Schedules;
    use bevy_ecs::world::World;
    use campfire_sim::SimUpdate;

    use crate::book::Book;

    /// A test's load of a book in place of the one its match's build put in.
    pub trait BookLoad: Book {
        /// Puts the book in place of the one the build put in `world`, as a test that loads a
        /// book alone does, and gives the script view what it copies of it, before the match's
        /// systems install, as their install reads the books (02-engine-core.md, A test's match
        /// installs at its first tick).
        fn load_into(self, world: &mut World) {
            Self::assert_open(world);
            world.insert_resource(self);
            Self::share_in(world);
        }

        /// Changes the book `world` holds by `change`, which reads the rest of the world, as a
        /// test that gives its match one more fact does, under the checks of `load_into`, so no
        /// change comes after the install that reads it.
        fn amend<R>(world: &mut World, change: impl FnOnce(&World, &mut Self) -> R) -> R {
            Self::assert_open(world);
            let mut book = world
                .remove_resource::<Self>()
                .expect("a declared book is in its world");
            let changed = change(world, &mut book);
            world.insert_resource(book);
            Self::share_in(world);
            changed
        }

        /// Asserts that `world` holds the book, as its module declares it, and that its match's
        /// systems have not installed, as their install reads the book.
        fn assert_open(world: &World) {
            assert!(
                world.contains_resource::<Self>(),
                "{} is declared by its module",
                type_name::<Self>()
            );
            let built = world.get_resource::<Schedules>();
            assert!(
                !built.is_some_and(|schedules| schedules.contains(SimUpdate)),
                "{} loads before its match's systems install",
                type_name::<Self>()
            );
        }
    }

    impl<B: Book> BookLoad for B {}
}

#[cfg(test)]
mod tests {
    use bevy_ecs::schedule::{Schedule, Schedules};
    use campfire_sim::SimUpdate;

    use super::*;
    use crate::book::internals::BookLoad;

    /// A book of a test, of a count.
    #[derive(Resource, Debug, PartialEq, Eq)]
    #[component(immutable)]
    struct Count(u32);

    impl Book for Count {
        const SCRIPTS_ONLY: bool = false;

        fn share(&self, _: &mut MatchNames) {}
    }

    /// A world that holds the count 1 and the names books share.
    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(Count(1));
        world.init_resource::<MatchNames>();
        world
    }

    /// A test changes a book before its match installs, and gets back what its change gives.
    #[test]
    fn an_amend_changes_a_book_before_the_install() {
        let mut world = world();
        let before = Count::amend(&mut world, |_, count| {
            count.0 += 1;
            count.0 - 1
        });
        assert_eq!((before, world.resource::<Count>()), (1, &Count(2)));
    }

    /// Once the match's systems installed, an amend panics, as the install read the book.
    #[test]
    #[should_panic(expected = "loads before its match's systems install")]
    fn an_amend_after_the_install_panics() {
        let mut world = world();
        world.init_resource::<Schedules>();
        world
            .resource_mut::<Schedules>()
            .insert(Schedule::new(SimUpdate));
        Count::amend(&mut world, |_, count| count.0 += 1);
    }
}
