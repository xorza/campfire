use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::{DeferredWorld, World};
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::name_script_arg;
use crate::scripts::state_decl::synced_state_decl::SyncTo;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::Units;
use crate::units::avatar::Avatar;
use crate::units::tag_book::TagBook;
use crate::units::type_scope::TypeScope;
use crate::units::unit_entity::UnitEntity;
use crate::units::unit_state::{SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_tags::UnitTags;
use crate::units::view::View;
use crate::values::names::NameId;

/// A unit's type, by its place in the match's unit types. Its tags, which it requires, come with
/// it on every path, a spawn, a restore and a client's replication, as its hook writes them.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[component(immutable, on_insert = UnitType::give_tags)]
#[require(UnitTags)]
#[serde(transparent)]
pub struct UnitType(u16);

impl UnitType {
    pub(crate) const fn new(index: u16) -> UnitType {
        UnitType(index)
    }

    /// Its place in the match's unit types, from 0.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The hook that gives a unit its type's tags, from the tag book, none in a match with none,
    /// in the place its type requires; and marks it an `Avatar` exactly when they hold `avatar`. Its
    /// modifiers' grants join them as stats derive them. A type the match did not load has no
    /// tags, and the restore check refuses it.
    fn give_tags(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let unit_type = *world
            .get::<UnitType>(entity)
            .expect("the hook's own component");
        let tags = UnitTags::at_spawn(world.get_resource::<TagBook>(), unit_type);
        let avatar = tags.tags.contains(Units::AVATAR.tag());
        *world
            .get_mut::<UnitTags>(entity)
            .expect("a unit type requires its tags") = tags;
        let marked = world.entity(entity).contains::<Avatar>();
        if avatar && !marked {
            world.commands().entity(entity).insert(Avatar);
        } else if !avatar && marked {
            world.commands().entity(entity).remove::<Avatar>();
        }
    }
}

impl NameId for UnitType {
    type Scope = TypeScope;
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> UnitType {
        UnitType(u16::try_from(index).expect("unit types fit u16"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl SimComponent for UnitType {
    const NAME: &'static str = "units.unit_type";
    type Host = UnitEntity;

    // A type the match did not load names nothing its books hold, and a unit without a class of
    // the state its type declares would read a field it lacks.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let known = world.non_send::<View>().names().types().contains(*self);
        let unit = world.entity(entity);
        let states = || {
            let book = world.resource::<UnitStateBook>();
            book.fields(*self).iter().all(|field| match field.sync {
                SyncTo::None => unit.contains::<UnitState<SyncNone>>(),
                SyncTo::Owner => unit.contains::<UnitState<SyncOwner>>(),
                SyncTo::Team => unit.contains::<UnitState<SyncTeam>>(),
                SyncTo::All => unit.contains::<UnitState<SyncAll>>(),
            })
        };
        known && states()
    }
}

impl Replication for UnitType {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}

// A unit type a script names is of the mode's scope: one of the mode's, or an avatar.
name_script_arg!(UnitType, NameKind::UnitType, |ctx, name| {
    ctx.view()
        .names()
        .unit_types()
        .named((TypeScope::Mode, name))
});
