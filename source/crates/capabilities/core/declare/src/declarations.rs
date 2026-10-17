use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::Component;
use bevy_ecs::event::Event;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::query::ROQueryItem;
use bevy_ecs::schedule::SystemCondition;
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use bevy_ecs::world::World;
use campfire_script::{ApiBuilder, CallParams, CallPart, RowFill, RowReads, ViewColumn};
use campfire_sim::{Breach, Capability, HostRule, OwnResources, SimComponent};
use campfire_state_types::StateTypes;
use campfire_values::EngineTag;

use crate::book::Book;
use crate::book_cx::{BuildFn, ExtendFn, RuleFn};
use crate::cache::Cache;
use crate::callbacks::Callbacks;
use crate::fact::Fact;
use crate::kind_spawn::KindSpawn;
use crate::kind_visitor::KindVisitor;
use crate::module_set::ModuleSet;
use crate::one_shot::OneShot;
use crate::report::Report;
use crate::role::Role;
use crate::scratch::Scratch;
use crate::section::Section;

/// How a column `C` of the script view fills from the parts `D` of each unit.
pub type ColumnFill<D, C> = for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>);

/// What reads the modules' declarations. Each module, the core's and each capability's, declares
/// through `Module::declare` all it adds to a match but its systems, the core its kinds of entity
/// among them, and every consumer reads the same list: the install, the script API's binding,
/// the network layer's lists of state types, and the declaration audit. No method has a default,
/// so a new category stops every consumer until it says what it does with it.
pub trait Declarations: StateTypes + KindVisitor + OwnResources {
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

    /// `B`, a book the module reads, which `build` makes from the match's packages before the
    /// install.
    fn book<B: Book>(&mut self, build: BuildFn<B>);

    /// `F`, a fact a module's books build on, which `build` makes from the match's packages.
    fn fact<F: Fact>(&mut self, build: BuildFn<F>);

    /// `extend`, which adds the module's part to `B`, a book or a fact of a module it builds on,
    /// as soon as `B` is built.
    fn extend<B: Send + Sync + 'static>(&mut self, extend: ExtendFn<B>);

    /// `rule`, a rule of the match's packages' data that the module reads, which the build of the
    /// books checks before it builds them, and whose error is the module's own.
    fn rule(&mut self, rule: RuleFn);

    /// `S`, a section of a data table that the module reads, by its key.
    fn section<S: Section>(&mut self);

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

    /// `observer`, which runs as each event `E` of `B` reaches it.
    fn observer<E: Event, B: Bundle, M>(&mut self, observer: impl IntoObserverSystem<E, B, M>);

    /// The removals of `C`, which a system of the module reads through `Removals<C>`, recorded as
    /// each happens and aged by the tick, alike on every host.
    fn removals<C: Component>(&mut self);

    /// `system`, which the module runs out of the schedule, by the id `R` keeps.
    fn one_shot<R: OneShot, M>(&mut self, system: impl IntoSystem<R::In, (), M> + 'static);

    /// `hook`, which runs as each `C` is inserted, as Bevy's component hook.
    fn on_insert<C: Component>(&mut self, hook: ComponentHook);

    /// The non-send resource `R`, which `build` makes of the world, as the install of the
    /// match's role finds it; none for a role that holds none, as a client holds no script host.
    fn non_send<R: 'static>(&mut self, build: fn(&World, &Role) -> Option<R>);

    /// The non-send resource `R`, which the module's load puts in from the books, not its
    /// install, as the mode's load puts in the mode it runs.
    fn non_send_loaded<R: 'static>(&mut self);

    /// `condition`, on which every stage of the tick and its start run: once it turns false, each
    /// stage from the next on runs no system, as the match's end stops them.
    fn stages_run_if<M>(&mut self, condition: impl SystemCondition<M> + Clone);

    /// `C`, a part the server alone computes, which every entity that holds `H` spawns with on a
    /// server's match, by Bevy's required component, its default; a client receives it from the
    /// server, or, of state only the server holds, never holds it, as it does not compute it.
    fn server_part<H: Component, C: Component + Default>(&mut self);
}
