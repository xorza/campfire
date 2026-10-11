use bevy_ecs::component::Component;
use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_sim::{Capability, HostRule, SimComponent};

use crate::declare::book::Book;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::state_types::StateTypes;
use crate::units::row_fill::RowFill;
use crate::units::row_parts::RowParts;
use crate::units::view_column::ViewColumn;

/// How a column `C` of the script view fills from the parts `D` of each unit.
pub(crate) type ColumnFill<D, C> = for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>);

/// What reads the modules' declarations. Each module, the core's and each capability's, declares
/// through `Declares::declare` all it adds to a match but its systems, and every consumer reads
/// the same list: the install, the script API's binding, the network layer's lists of state
/// types, and the declaration audit. No method has a default, so a new category stops every
/// consumer until it says what it does with it.
pub(crate) trait Declarations: StateTypes {
    /// Starts the declarations of `module`: the core's, `None`, or a capability's, the mode's
    /// among them.
    fn module(&mut self, module: Option<Capability>);

    /// `K`, a kind of entity, which state types name as their host.
    fn kind<K: HostRule + Component>(&mut self);

    /// Every entity that holds `H` holds `C`, a declared state type, as a restore checks.
    fn require<H: Component, C: SimComponent>(&mut self);

    /// `C`, a declared state type of a crate that knows no kinds, sits on the entities of `H`
    /// alone, as a restore checks.
    fn host<C: SimComponent, H: HostRule>(&mut self);

    /// `check`, a rule of `C`, a declared state type, that a module other than its owner knows,
    /// which a restore runs.
    fn check<C: SimComponent>(&mut self, check: fn(&World) -> bool);

    /// `B`, a book the module reads, empty until the books fill it.
    fn book<B: Book>(&mut self);

    /// `C`, a column of the script view, which `fill` fills from each unit's parts `D`.
    fn column<C: ViewColumn + Default, D: RowParts>(&mut self, fill: ColumnFill<D, C>);

    /// `P`, a part of every script call.
    fn call_part<P: CallPart + Default>(&mut self);

    /// `C`, a part of each player's entity, which `start` makes as the entity spawns; a restore
    /// checks that every player's entity holds one.
    fn player_part<C: SimComponent>(&mut self, start: fn(&World) -> C);

    /// The module's script API, which `register` binds and records.
    fn api(&mut self, register: fn(&mut ApiBuilder<'_>));
}
