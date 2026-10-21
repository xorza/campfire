use bevy_ecs::resource::Resource;
use campfire_actions::{ActionsData, ByAction};
use campfire_declarations::{Book, BookCx, BookError};
use campfire_script::MatchNames;
use campfire_stats::{ModifierAppliers, Way};
use campfire_values::ModifierId;

use crate::attack_data::AttackData;
use crate::weapon::Weapon;

/// The modifiers of every stage of every streak weapon, each once, in the order of their ids,
/// which a unit's streak grants or releases (04-capabilities/combat.md, A shot streak gives a
/// stage's modifier). Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct StreakBook(Box<[ModifierId]>);

impl StreakBook {
    /// The stages' modifiers of the weapons of the match.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<Self, BookError> {
        Ok(Self::of(cx.book::<ByAction<Weapon>>()?))
    }

    /// The stages' modifiers of `weapons`.
    fn of(weapons: &ByAction<Weapon>) -> Self {
        let mut modifiers: Vec<ModifierId> = weapons
            .values()
            .filter_map(|weapon| weapon.streak.as_ref())
            .flat_map(|streak| streak.stages.iter().map(|stage| stage.modifier))
            .collect();
        modifiers.sort_unstable();
        modifiers.dedup();
        Self(modifiers.into())
    }

    /// Adds that each package's streak weapons apply the modifiers of their stages, with no
    /// action, as a unit's streak grants them, read from the actions' data, as the appliers come
    /// before the action book; an error for a modifier its package does not have comes with the
    /// weapons' book.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn applied_ways(
        cx: &BookCx<'_>,
        appliers: &mut ModifierAppliers,
    ) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>().into_iter();
            for (_, action) in actions.flat_map(|actions| &actions.0) {
                let streak = action
                    .section::<AttackData>()
                    .and_then(|data| data.streak.as_ref());
                for stage in streak.into_iter().flat_map(|streak| &streak.stages) {
                    appliers.apply(package, stage.modifier.as_str(), Way::NoAction);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = ModifierId> + '_ {
        self.0.iter().copied()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Book for StreakBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_actions::ByAction;
    use campfire_declarations::internals::BookLoad;

    use crate::streak_book::StreakBook;
    use crate::weapon::Weapon;

    impl StreakBook {
        /// Builds the book again from the weapons of `world`, as the load builds it after the
        /// weapons' book: for a test that adds a weapon.
        pub(crate) fn load(world: &mut World) {
            Self::of(world.resource::<ByAction<Weapon>>()).load_into(world);
        }
    }
}
