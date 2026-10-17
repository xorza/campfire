//! The declaration audit: a consumer of every module's declarations, which checks what no single
//! declaration can, each a rule between declarations.

use std::any::{TypeId, type_name};
use std::fmt::Debug;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::event::Event;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::SystemCondition;
use bevy_ecs::schedule::{InternedSystemSet, SystemSet};
use bevy_ecs::system::{IntoObserverSystem, IntoSystem, System};
use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, CastSpends, DeliveryShape};
use campfire_combat::ClipShots;
use campfire_common::EnumAll;
use campfire_declare::{
    Book, BuildFn, Cache, Callbacks, ColumnFill, Declarations, EntityKind, ExtendFn, Fact,
    KindSpawn, KindVisitor, Listed, ModuleSet, OneShot, Report, Role, RuleFn, Scratch, Section,
    SetPlace,
};
use campfire_deliveries::Deliverers;
use campfire_navigation::{RoomMakingRun, StaticTrackingRun};
use campfire_players::Player;
use campfire_production::{Gatherer, PlayerSupply, Site, TrainQueue};
use campfire_script::{ApiBuilder, CallParams, CallPart, MatchNames, RowReads, ViewColumn};
use campfire_sim::{
    Breach, Capability, HostRule, OnAny, OwnResources, SimComponent, SimResource, SimSet,
    Unpredicted,
};
use campfire_state_types::{DataKind, NotSent, Replication, StateTypes};
use campfire_stats::Modifiers;
use campfire_units::{BodyUnit, Dead, DeliveryEntity, OpenCellRun, Owner, SeenBy, UnitEntity};
use campfire_values::{EngineTag, NameId, TagProperties, UnitType};
use campfire_vision::VisionColumn;
use serde::{Deserialize, Serialize};

use crate::release::Release;

/// A type, by its id, with its name for the problems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Named {
    id: TypeId,
    name: &'static str,
}

impl Named {
    fn of<T: 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
        }
    }
}

/// The kinds `H` takes, each by its marker's id; none when it takes any entity.
fn kinds_of<H: HostRule>() -> Option<Vec<TypeId>> {
    let mut kinds = Vec::new();
    H::kinds(&mut kinds).then_some(kinds)
}

/// A state type as declared: its type and the kinds its host takes.
#[derive(Debug)]
struct State {
    ty: Named,
    kinds: Option<Vec<TypeId>>,
}

/// A book as declared: its type, and the capability of the module that declares it, its owner.
#[derive(Debug)]
struct DeclaredBook {
    ty: Named,
    declared_by: Option<Capability>,
}

/// A callback as declared: its registry, its key as `Debug` writes it, the capability whose
/// module owns the key, and the capability of the module that declares it.
#[derive(Debug)]
struct DeclaredCallback {
    registry: Named,
    key: String,
    owner: Option<Capability>,
    declared_by: Option<Capability>,
}

/// A system set as declared.
#[derive(Debug)]
struct DeclaredSet {
    set: InternedSystemSet,
}

/// Every declaration of the modules it visits.
#[derive(Debug, Default)]
struct DeclarationAudit {
    /// The capability of the module whose declarations it visits.
    capability: Option<Capability>,
    kinds: Vec<TypeId>,
    /// Each kind's parts of its states, by the kind.
    kind_states: Vec<(Named, Named)>,
    states: Vec<State>,
    resources: Vec<&'static str>,
    requires: Vec<(Named, Named)>,
    hosts: Vec<(Named, Option<Vec<TypeId>>)>,
    checks: Vec<Named>,
    books: Vec<DeclaredBook>,
    columns: Vec<Named>,
    call_parts: Vec<Named>,
    /// Each part: its host, the kinds the host takes, and its type.
    parts: Vec<(Named, Vec<TypeId>, Named)>,
    calls: Vec<Named>,
    callbacks: Vec<DeclaredCallback>,
    sets: Vec<DeclaredSet>,
    /// The types the match derives again, which no state holds.
    derived: Vec<TypeId>,
    /// Each component a column reads, by type and name.
    column_reads: Vec<(TypeId, &'static str)>,
    /// The engine tags, in the order the modules declare them.
    engine_tags: Vec<EngineTag>,
    /// Each observer, by its system's name.
    observers: Vec<String>,
    /// Each part whose removals a module reads, by its type.
    removals: Vec<Named>,
    /// Each system run out of the schedule, by the resource that keeps its id.
    one_shots: Vec<Named>,
    /// Each part a server's match derives: its holder, and its type.
    server_parts: Vec<(Named, Named)>,
    /// Each resource the match's build puts in itself.
    owns: Vec<Named>,
}

impl DeclarationAudit {
    fn state<C: SimComponent>(&mut self) {
        self.states.push(State {
            ty: Named::of::<C>(),
            kinds: kinds_of::<C::Host>(),
        });
    }

    fn state_of(&self, id: TypeId) -> Option<&State> {
        self.states.iter().find(|state| state.ty.id == id)
    }

    /// What breaks a rule between the declarations, each by the type it names: every type is
    /// declared once; each state type sits on declared kinds, by its host, by a `host`
    /// declaration when its host takes any entity, or as a kind itself; a pair's holder is a kind
    /// or a state type and its part a state type; a check and a `host` name state types; a book's
    /// capability is the module's that declares it; a kind's part is a state type of that kind; a
    /// callback is its key's owner's, declared once, in a registry a module calls; an engine tag
    /// is at its number, in the order the modules declare them, under a name of its own; and a
    /// column reads only declared types, as a restored match's scripts would read another value of one
    /// no state or derivation holds.
    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut once = |what: &str, names: Vec<&'static str>| {
            for (at, name) in names.iter().enumerate() {
                if names[..at].contains(name) {
                    problems.push(format!("{what} {name} is declared twice"));
                }
            }
        };
        once(
            "state",
            self.states.iter().map(|state| state.ty.name).collect(),
        );
        once("resource", self.resources.clone());
        once("run", self.one_shots.iter().map(|ty| ty.name).collect());
        once("own", self.owns.iter().map(|ty| ty.name).collect());
        once("book", self.books.iter().map(|book| book.ty.name).collect());
        once("column", self.columns.iter().map(|ty| ty.name).collect());
        once(
            "call part",
            self.call_parts.iter().map(|ty| ty.name).collect(),
        );
        once("part", self.parts.iter().map(|(.., ty)| ty.name).collect());
        // A state part sits on a declared kind, and never comes with the spawn as well.
        for (kind, state) in &self.kind_states {
            if !self.kinds.contains(&kind.id) {
                problems.push(format!(
                    "{} is a state of {}, no kind",
                    state.name, kind.name
                ));
            }
            if self.parts.iter().any(|(.., part)| part.id == state.id) {
                problems.push(format!("{} is a state and a part of a spawn", state.name));
            }
        }
        let kinds_declared = |kinds: &[TypeId]| kinds.iter().all(|kind| self.kinds.contains(kind));
        for state in &self.states {
            let hosted = self.hosts.iter().find(|(ty, _)| ty.id == state.ty.id);
            let kinds = state.kinds.as_ref().or_else(|| hosted?.1.as_ref());
            let sits = match kinds {
                Some(kinds) => kinds_declared(kinds),
                None => self.kinds.contains(&state.ty.id),
            };
            if !sits {
                problems.push(format!("{} sits on no declared kind", state.ty.name));
            }
        }
        for (holder, part) in &self.requires {
            let holds = self.kinds.contains(&holder.id) || self.state_of(holder.id).is_some();
            if !holds || self.state_of(part.id).is_none() {
                problems.push(format!(
                    "{} needs {}, not both declared",
                    holder.name, part.name
                ));
            }
        }
        for (ty, kinds) in &self.hosts {
            let declared = kinds.as_deref().is_some_and(kinds_declared);
            if self.state_of(ty.id).is_none() || !declared {
                problems.push(format!("{} is hosted on what is not declared", ty.name));
            }
        }
        for ty in &self.checks {
            if self.state_of(ty.id).is_none() {
                problems.push(format!("a check of {}, not declared", ty.name));
            }
        }
        for (host, kinds, ty) in &self.parts {
            let of_host = self.state_of(ty.id).is_some_and(|state| {
                let declared = || {
                    let (_, kinds) = self.hosts.iter().find(|(hosted, _)| hosted.id == ty.id)?;
                    kinds.as_deref()
                };
                let held = state.kinds.as_deref().or_else(declared).unwrap_or_default();
                kinds.iter().all(|kind| held.contains(kind))
            });
            if !of_host {
                problems.push(format!("{} is no state of {}", ty.name, host.name));
            }
        }
        self.engine_tag_problems(&mut problems);
        for &(id, name) in &self.column_reads {
            if self.state_of(id).is_none() && !self.derived.contains(&id) {
                problems.push(format!("a column reads {name}, not declared"));
            }
        }
        for (holder, part) in &self.server_parts {
            let holds = self.kinds.contains(&holder.id) || self.state_of(holder.id).is_some();
            let derived = self.state_of(part.id).is_some() || self.derived.contains(&part.id);
            if !holds || !derived {
                problems.push(format!(
                    "{} is a server part of {}, not both declared",
                    part.name, holder.name
                ));
            }
        }
        self.callback_problems(&mut problems);
        self.set_problems(&mut problems);
        problems
    }

    /// Each system set declared twice: a module's sets are its own by its type.
    fn set_problems(&self, problems: &mut Vec<String>) {
        for (at, declared) in self.sets.iter().enumerate() {
            let set = declared.set;
            if self.sets[..at].iter().any(|other| other.set == set) {
                problems.push(format!("the set {set:?} is declared twice"));
            }
        }
    }

    /// Each engine tag not at its number, in the order the modules declare them, and each name
    /// declared twice.
    fn engine_tag_problems(&self, problems: &mut Vec<String>) {
        for (at, tag) in self.engine_tags.iter().enumerate() {
            if tag.tag().index() != at {
                problems.push(format!(
                    "the engine tag {} is not at its number",
                    tag.name()
                ));
            }
            if self.engine_tags[..at]
                .iter()
                .any(|other| other.name() == tag.name())
            {
                problems.push(format!("the engine tag {} is declared twice", tag.name()));
            }
        }
    }

    /// Each callback not its key's owner's, declared twice, or in a registry no module calls.
    fn callback_problems(&self, problems: &mut Vec<String>) {
        for (at, callback) in self.callbacks.iter().enumerate() {
            let (registry, key) = (callback.registry.name, &callback.key);
            if callback.owner != callback.declared_by {
                problems.push(format!(
                    "the callback {key} of {registry} is declared by another module"
                ));
            }
            let twice = self.callbacks[..at]
                .iter()
                .any(|other| other.registry.id == callback.registry.id && other.key == *key);
            if twice {
                problems.push(format!(
                    "the callback {key} of {registry} is declared twice"
                ));
            }
            if !self
                .calls
                .iter()
                .any(|calls| calls.id == callback.registry.id)
            {
                problems.push(format!("{registry} has a callback no module calls"));
            }
        }
    }
}

impl StateTypes for DeclarationAudit {
    fn component<C: Replication>(&mut self) {
        self.state::<C>();
    }

    fn derived<C: Component>(&mut self) {
        self.derived.push(TypeId::of::<C>());
    }

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.state::<C>();
    }

    fn resource<R: SimResource + Default>(&mut self) {
        self.resources.push(R::NAME);
    }

    fn sim_component<C: Replication>(&mut self) {
        self.state::<C>();
    }
}

impl KindVisitor for DeclarationAudit {
    fn kind<K: EntityKind>(&mut self) {
        self.kinds.push(TypeId::of::<K>());
    }

    fn state<K: EntityKind, S: Component>(&mut self) {
        self.kind_states.push((Named::of::<K>(), Named::of::<S>()));
    }
}

impl Declarations for DeclarationAudit {
    fn module(&mut self, capability: Option<Capability>) {
        self.capability = capability;
    }

    fn require<H: SimComponent, C: SimComponent>(&mut self) {
        self.requires.push((Named::of::<H>(), Named::of::<C>()));
    }

    fn host<C: SimComponent, H: HostRule>(&mut self) {
        self.hosts.push((Named::of::<C>(), kinds_of::<H>()));
    }

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {
        self.checks.push(Named::of::<C>());
    }

    fn fact<F: Fact>(&mut self, _: BuildFn<F>) {}

    fn extend<B: Send + Sync + 'static>(&mut self, _: ExtendFn<B>) {}

    fn rule(&mut self, _: RuleFn) {}

    fn book<B: Book>(&mut self, _: BuildFn<B>) {
        self.books.push(DeclaredBook {
            ty: Named::of::<B>(),
            declared_by: self.capability,
        });
    }

    fn section<S: Section>(&mut self) {}

    fn listed<L: Listed>(&mut self) {}

    fn sets<S: ModuleSet>(&mut self) {
        for &set in S::ALL {
            self.sets.push(DeclaredSet { set: set.intern() });
        }
    }

    fn scratch<R: Scratch>(&mut self) {
        self.resources.push(type_name::<R>());
    }

    fn cache<R: Cache>(&mut self) {
        self.resources.push(type_name::<R>());
    }

    fn report<R: Report>(&mut self) {
        self.resources.push(type_name::<R>());
    }

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {
        self.columns.push(Named::of::<C>());
        D::components(&mut self.column_reads);
    }

    fn call_part<P: CallPart + Default>(&mut self) {
        self.call_parts.push(Named::of::<P>());
    }

    fn call_params<P: CallParams + Default>(&mut self) {
        self.call_parts.push(Named::of::<P>());
    }

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        _: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
        let kinds = kinds_of::<H>().expect("a part's host names its kinds");
        self.parts.push((Named::of::<H>(), kinds, Named::of::<C>()));
    }

    fn calls<C: Callbacks>(&mut self) {
        self.calls.push(Named::of::<C>());
    }

    fn callback<C: Callbacks>(&mut self, key: C::Key, _: C::Callback) {
        self.callbacks.push(DeclaredCallback {
            registry: Named::of::<C>(),
            key: format!("{key:?}"),
            owner: C::owner(key),
            declared_by: self.capability,
        });
    }

    fn engine_tags(&mut self, tags: &'static [EngineTag]) {
        self.engine_tags.extend_from_slice(tags);
    }

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}

    fn observer<E: Event, B: Bundle, M>(&mut self, observer: impl IntoObserverSystem<E, B, M>) {
        let system = IntoObserverSystem::into_system(observer);
        self.observers.push(system.name().shortname().to_string());
    }

    fn removals<C: Component>(&mut self) {
        self.removals.push(Named::of::<C>());
    }

    fn one_shot<R: OneShot, M>(&mut self, _: impl IntoSystem<R::In, R::Out, M> + 'static) {
        self.one_shots.push(Named::of::<R>());
    }

    fn loaded<R: Resource>(&mut self) {}

    fn stages_run_if<M>(&mut self, _: impl SystemCondition<M> + Clone) {}

    fn server_part<H: Component, C: Component + Default>(&mut self) {
        self.server_parts.push((Named::of::<H>(), Named::of::<C>()));
    }

    fn on_insert<C: Component>(&mut self, _: ComponentHook) {}

    fn non_send<R: 'static>(&mut self, _: fn(&World, &Role) -> Option<R>) {}
}

impl OwnResources for DeclarationAudit {
    fn own<R: Resource>(&mut self) {
        self.owns.push(Named::of::<R>());
    }
}

#[test]
fn the_modules_declarations_keep_every_rule_between_them() {
    let mut audit = DeclarationAudit::default();
    Release::declare(Release::capabilities(), &mut audit);
    assert_eq!(audit.problems(), Vec::<String>::new());
    // The audit read every module: the core's four kinds, the stats' and the players' parts of a
    // player, the mode's three of the match, the 30 of a unit, items' `UseGroup`, orders'
    // `NextThink`, production's `StatusTags` and combat's `TurretAngle` among them, and the 54
    // books, 14 of the core's, interaction's two, combat's armors, turrets and weapons, and
    // production's trains and gathers among them.
    assert_eq!(audit.kinds.len(), 4);
    assert_eq!(audit.parts.len(), 35);
    assert_eq!(audit.books.len(), 54);
    let core = audit.books.iter().filter(|book| book.declared_by.is_none());
    assert_eq!(core.count(), 14);
    // The observers, the runs out of the schedule and the parts a server derives, which the
    // installer adds for their modules.
    assert_eq!(
        audit.observers,
        [
            "UnitTracker<GridUnits>::aged",
            "UnitTracker<GridUnits>::lost_body",
            "Units::refill_view",
            "Level::raise",
            "HeldHits::drop_died",
            "UnitTracker<StaticBodies>::aged",
            "StaticBodies::part_removed",
            "StaticBodies::state_inserted",
            "StaticBodies::state_removed",
            "Flying::lose_teleported",
            "Flying::lose_died",
            "ProductionOrders::ordered",
            "UseOrders::ordered",
        ]
    );
    // The parts whose removals a module reads, which reach it as the tick ages them.
    let removals: Vec<&str> = audit.removals.iter().map(|ty| ty.name).collect();
    assert_eq!(
        removals,
        [
            type_name::<Unpredicted>(),
            type_name::<Dead>(),
            type_name::<Modifiers>(),
            type_name::<Owner>(),
            type_name::<Dead>(),
            type_name::<Unpredicted>(),
            type_name::<Dead>(),
            type_name::<Gatherer>(),
            type_name::<Owner>(),
            type_name::<Site>(),
            type_name::<TrainQueue>(),
            type_name::<UnitType>(),
        ]
    );
    let runs: Vec<&str> = audit.one_shots.iter().map(|ty| ty.name).collect();
    assert_eq!(
        runs,
        [
            type_name::<StaticTrackingRun>(),
            type_name::<RoomMakingRun>(),
            type_name::<OpenCellRun>()
        ]
    );
    let server: Vec<(&str, &str)> = audit
        .server_parts
        .iter()
        .map(|(holder, part)| (holder.name, part.name))
        .collect();
    assert_eq!(
        server,
        [
            (type_name::<ActionSlots>(), type_name::<ClipShots>()),
            (type_name::<UnitEntity>(), type_name::<SeenBy>()),
            (type_name::<Player>(), type_name::<PlayerSupply>()),
        ]
    );
    // The engine tags keep their numbers while the module list keeps its order.
    let engine: Vec<&str> = audit.engine_tags.iter().map(|tag| tag.name()).collect();
    assert_eq!(
        engine,
        [
            "avatar",
            "dead",
            "projectile",
            "area",
            "constructing",
            "node",
            "drop_off",
            "gathering"
        ]
    );
}

/// A state type of no kind, which a test declares.
#[derive(Component, Debug, Serialize, Deserialize)]
struct Stray;

impl SimComponent for Stray {
    const NAME: &'static str = "test.stray";
    type Host = OnAny;

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Stray {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

/// Two engine tags a test declares: the first not at its number, and the second of its name.
const STRAY_TAGS: [EngineTag; 2] = [
    EngineTag::new(1, "stray", TagProperties::NONE),
    EngineTag::new(1, "stray", TagProperties::NONE),
];

/// A component no module declares, which a test's column reads and a test's server part is.
#[derive(Component, Debug, Default)]
struct Undeclared;

/// A book a test declares, as the stats' own.
#[derive(Resource, Debug, Default)]
struct StrayBook;

impl Book for StrayBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

/// Sets of no module, which the test declares twice.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
enum StraySet {
    Early,
    Late,
}

impl ModuleSet for StraySet {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        SetPlace::stage(match self {
            Self::Early => SimSet::Inputs,
            Self::Late => SimSet::Hit,
        })
    }
}

#[test]
fn each_rule_names_the_declaration_that_breaks_it() {
    // One module of the combat capability: a state type on no kind, declared twice, needed by a
    // unit though it is a part that sits nowhere, checked, and put on players; a book the stats
    // own; a kind's part, a unit's, needing a type no module declares; a unit's server part, of a
    // type no module declares; and sets declared twice.
    let mut audit = DeclarationAudit::default();
    audit.module(None);
    audit.kind::<BodyUnit>();
    audit.kind::<DeliveryEntity>();
    audit.component::<UnitEntity>();
    audit.module(Some(Capability::Combat));
    audit.component::<Stray>();
    audit.component::<Stray>();
    audit.require::<UnitEntity, Stray>();
    audit.require::<UnitEntity, Player>();
    audit.check::<Player>(|_| Ok(()));
    audit.book::<StrayBook>(|_| Ok(StrayBook));
    audit.book::<StrayBook>(|_| Ok(StrayBook));
    audit.part::<Player, Stray>(|_| Some(Stray));
    audit.engine_tags(&STRAY_TAGS);
    // A column that reads a type no module declares.
    let reads_undeclared: ColumnFill<&Undeclared, VisionColumn> = |_, _| {};
    audit.column::<VisionColumn, &Undeclared>(reads_undeclared);
    audit.server_part::<UnitEntity, Undeclared>();
    // A callback the areas own, twice, in a registry no module calls.
    audit.callback::<Deliverers>(Capability::Areas, |_, _, _, _| {});
    audit.callback::<Deliverers>(Capability::Areas, |_, _, _, _| {});
    // Sets declared twice.
    audit.sets::<StraySet>();
    audit.sets::<StraySet>();
    let queues = type_name::<Deliverers>();
    let stray = type_name::<Stray>();
    let player = type_name::<Player>();
    let unit = type_name::<UnitEntity>();
    assert_eq!(
        audit.problems(),
        [
            format!("state {stray} is declared twice"),
            format!("book {} is declared twice", type_name::<StrayBook>()),
            format!("{stray} sits on no declared kind"),
            format!("{stray} sits on no declared kind"),
            format!("{unit} needs {player}, not both declared"),
            format!("a check of {player}, not declared"),
            format!("{stray} is no state of {player}"),
            "the engine tag stray is not at its number".to_owned(),
            "the engine tag stray is declared twice".to_owned(),
            format!("a column reads {}, not declared", type_name::<Undeclared>()),
            format!(
                "{} is a server part of {unit}, not both declared",
                type_name::<Undeclared>()
            ),
            format!("the callback Areas of {queues} is declared by another module"),
            format!("{queues} has a callback no module calls"),
            format!("the callback Areas of {queues} is declared by another module"),
            format!("the callback Areas of {queues} is declared twice"),
            format!("{queues} has a callback no module calls"),
            "the set Early is declared twice".to_owned(),
            "the set Late is declared twice".to_owned(),
        ]
    );
    // A `host` declaration puts the stray on a declared kind.
    audit.host::<Stray, UnitEntity>();
    assert!(
        !audit
            .problems()
            .contains(&format!("{stray} sits on no declared kind"))
    );
}

#[test]
fn each_key_a_caller_asks_for_has_its_callback() {
    // Each kind of delivery sets out by its capability's callback, and a cast spends what items
    // keep of a slot.
    let mut audit = DeclarationAudit::default();
    Release::declare(Release::capabilities(), &mut audit);
    let declared = |registry: &str, key: String| {
        audit
            .callbacks
            .iter()
            .any(|callback| callback.registry.name == registry && callback.key == key)
    };
    for capability in DeliveryShape::CAPABILITIES {
        assert!(declared(
            type_name::<Deliverers>(),
            format!("{capability:?}")
        ));
    }
    assert!(declared(type_name::<CastSpends>(), format!("{:?}", ())));
}
