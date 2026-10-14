use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_script::{ApiBuilder, CallParams, CallPart, RowFill, RowReads, ViewColumn};
use campfire_sim::{Breach, Capability, HostRule, SimComponent};
use campfire_state_types::StateTypes;
use campfire_values::EngineTag;

use crate::declare::book::Book;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::kind_spawn::KindSpawn;
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::module_set::ModuleSet;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;

/// How a column `C` of the script view fills from the parts `D` of each unit.
pub type ColumnFill<D, C> = for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>);

/// What reads the modules' declarations. Each module, the core's and each capability's, declares
/// through `Module::declare` all it adds to a match but its systems, the core its kinds of entity
/// among them, and every consumer reads the same list: the install, the script API's binding,
/// the network layer's lists of state types, and the declaration audit. No method has a default,
/// so a new category stops every consumer until it says what it does with it.
pub trait Declarations: StateTypes + KindVisitor {
    /// Starts the declarations of the module of `capability`: the core's, `None`, or a
    /// capability's, the mode's among them.
    fn module(&mut self, capability: Option<Capability>);

    /// Every entity that holds `H` holds `C`, a declared state type, as a restore checks.
    fn require<H: SimComponent, C: SimComponent>(&mut self);

    /// `C`, a declared state type of a crate that knows no kinds, sits on the entities of `H`
    /// alone, as a restore checks.
    fn host<C: SimComponent, H: HostRule>(&mut self);

    /// `check`, a rule of `C`, a declared state type, that a module other than its owner knows,
    /// which a restore runs.
    fn check<C: SimComponent>(&mut self, check: fn(&World) -> Result<(), Breach>);

    /// `B`, a book the module reads, empty until the books fill it.
    fn book<B: Book>(&mut self);

    /// `S`, the module's system sets, each with its place and its order.
    fn sets<S: ModuleSet>(&mut self);

    /// `R`, a resource a tick fills and empties.
    fn scratch<R: Scratch>(&mut self);

    /// `R`, a resource built again from the match's state, its map or its install.
    fn cache<R: Cache>(&mut self);

    /// `R`, a tick's record for the host.
    fn report<R: Report>(&mut self);

    /// `C`, a registry of callbacks the module calls.
    fn calls<C: Callbacks>(&mut self);

    /// `callback`, under `key`, in the registry of callbacks `C`.
    fn callback<C: Callbacks>(&mut self, key: C::Key, callback: C::Callback);

    /// `C`, a column of the script view, which `fill` fills from each unit's parts `D`.
    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, fill: ColumnFill<D, C>);

    /// `P`, a part of every script call.
    fn call_part<P: CallPart + Default>(&mut self);

    /// `P`, a part of every script call that holds the params of its scope's calls.
    fn call_params<P: CallParams + Default>(&mut self);

    /// `C`, a part of each entity of the host `H`, a kind or every unit, which `start` makes as
    /// the entity spawns, when it gives one; a restore checks that each entity of the host holds
    /// each part its spawn would give it.
    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        start: fn(&KindSpawn<'_, H>) -> Option<C>,
    );

    /// `tags`, the module's engine tags, which the tag book places after those declared before
    /// them: each at its number, as the module list gives the numbers.
    fn engine_tags(&mut self, tags: &'static [EngineTag]);

    /// The module's script API, which `register` binds and records.
    fn api(&mut self, register: fn(&mut ApiBuilder<'_>));
}
