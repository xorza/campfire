//! The declaration audit: a consumer of every module's declarations, which checks what no single
//! declaration can, each a rule between declarations.

use std::any::{TypeId, type_name};
use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, OnAny, SimComponent, SimResource};
use serde::{Deserialize, Serialize};

use crate::abilities::cast_spends::CastSpends;
use crate::actions::capability_does_spec::CapabilityDoesSpec;
use crate::actions::effect_queues::{EffectQueues, QueueListed};
use crate::declare::book::Book;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::kind_spawn::KindSpawn;
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;
use crate::deliveries::deliverers::{Deliverers, DeliveryKind};
use crate::players::player::Player;
use crate::release::Release;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::scripts::frame::Frame;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;
use crate::state_types::{Declared, StateTypes};
use crate::units::body_unit::BodyUnit;
use crate::units::delivery_entity::DeliveryEntity;
use crate::units::engine_tag::EngineTag;
use crate::units::entity_kind::EntityKind;
use crate::units::row_reads::RowReads;
use crate::units::tag_properties::TagProperties;
use crate::units::unit_entity::UnitEntity;
use crate::units::view::View;
use crate::units::view_column::ViewColumn;
use crate::vision::vision_column::VisionColumn;

/// A type, by its id, with its name for the problems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Named {
    id: TypeId,
    name: &'static str,
}

impl Named {
    fn of<T: 'static>() -> Named {
        Named {
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

/// A book as declared: its type, its owner, and the capability of the module that declares it.
#[derive(Debug)]
struct DeclaredBook {
    ty: Named,
    owner: Option<Capability>,
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

/// Every declaration of the modules it visits.
#[derive(Debug, Default)]
struct DeclarationAudit {
    /// The capability of the module whose declarations it visits.
    capability: Option<Capability>,
    kinds: Vec<TypeId>,
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
    /// The types the match derives again, which no state holds.
    derived: Vec<TypeId>,
    /// Each component a column reads, by type and name.
    column_reads: Vec<(TypeId, &'static str)>,
    /// The engine tags, in the order the modules declare them.
    engine_tags: Vec<EngineTag>,
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
        once("book", self.books.iter().map(|book| book.ty.name).collect());
        once("column", self.columns.iter().map(|ty| ty.name).collect());
        once(
            "call part",
            self.call_parts.iter().map(|ty| ty.name).collect(),
        );
        once("part", self.parts.iter().map(|(.., ty)| ty.name).collect());
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
        for book in &self.books {
            if book.owner != book.declared_by {
                problems.push(format!("{} is declared by another module", book.ty.name));
            }
        }
        for (host, kinds, ty) in &self.parts {
            let of_host = self.state_of(ty.id).is_some_and(|state| {
                let held = state.kinds.as_deref().unwrap_or_default();
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
        self.callback_problems(&mut problems);
        problems
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
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.state::<C>();
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.state::<C>();
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
        self.state::<C>();
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.state::<C>();
    }

    fn server<C: Replication>(&mut self, _: Declared) {
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

    fn book<B: Book>(&mut self) {
        self.books.push(DeclaredBook {
            ty: Named::of::<B>(),
            owner: B::OWNER,
            declared_by: self.capability,
        });
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
}

#[test]
fn the_modules_declarations_keep_every_rule_between_them() {
    let mut audit = DeclarationAudit::default();
    Release::declare(Release::capabilities(), &mut audit);
    assert_eq!(audit.problems(), Vec::<String>::new());
    // The audit read every module: the core's four kinds, the stats' and the players' parts of a
    // player, the mode's three of the match, the 24 of a unit, and the 40 books, 12 of the core's.
    assert_eq!(audit.kinds.len(), 4);
    assert_eq!(audit.parts.len(), 29);
    assert_eq!(audit.books.len(), 40);
    let core = audit.books.iter().filter(|book| book.owner.is_none());
    assert_eq!(core.count(), 12);
    // The engine tags keep their numbers while the module list keeps its order.
    let engine: Vec<&str> = audit.engine_tags.iter().map(|tag| tag.name()).collect();
    assert_eq!(
        engine,
        [
            "avatar",
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

/// A component no module declares, which a test's column reads.
#[derive(Component, Debug)]
struct Undeclared;

/// A book a test declares, as the stats' own.
#[derive(Resource, Debug, Default)]
struct StrayBook;

impl Book for StrayBook {
    const OWNER: Option<Capability> = Some(Capability::Stats);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View, _: Option<&mut Frame>) {}
}

#[test]
fn each_rule_names_the_declaration_that_breaks_it() {
    // One module of the combat capability: a state type on no kind, declared twice, needed by a
    // unit though it is a part that sits nowhere, checked, and put on players; a book the stats
    // own; and a kind's part, a unit's, needing a type no module declares.
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
    audit.book::<StrayBook>();
    audit.part::<Player, Stray>(|_| Some(Stray));
    audit.engine_tags(&STRAY_TAGS);
    // A column that reads a type no module declares.
    let reads_undeclared: ColumnFill<&Undeclared, VisionColumn> = |_, _| {};
    audit.column::<VisionColumn, &Undeclared>(reads_undeclared);
    // A callback the areas own, twice, in a registry no module calls.
    let queue: QueueListed = |_, _, _, _, _| Ok(());
    audit.callback::<EffectQueues>(Capability::Areas, queue);
    audit.callback::<EffectQueues>(Capability::Areas, queue);
    let queues = type_name::<EffectQueues>();
    let stray = type_name::<Stray>();
    let player = type_name::<Player>();
    let unit = type_name::<UnitEntity>();
    assert_eq!(
        audit.problems(),
        [
            format!("state {stray} is declared twice"),
            format!("{stray} sits on no declared kind"),
            format!("{stray} sits on no declared kind"),
            format!("{unit} needs {player}, not both declared"),
            format!("a check of {player}, not declared"),
            format!("{} is declared by another module", type_name::<StrayBook>()),
            format!("{stray} is no state of {player}"),
            "the engine tag stray is not at its number".to_owned(),
            "the engine tag stray is declared twice".to_owned(),
            format!("a column reads {}, not declared", type_name::<Undeclared>()),
            format!("the callback Areas of {queues} is declared by another module"),
            format!("{queues} has a callback no module calls"),
            format!("the callback Areas of {queues} is declared by another module"),
            format!("the callback Areas of {queues} is declared twice"),
            format!("{queues} has a callback no module calls"),
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
    // A listed effect queues in the capability it names; each kind of delivery sets out by its
    // capability's callback; and a cast spends what items keep of a slot.
    let mut audit = DeclarationAudit::default();
    Release::declare(Release::capabilities(), &mut audit);
    let declared = |registry: &str, key: String| {
        audit
            .callbacks
            .iter()
            .any(|callback| callback.registry.name == registry && callback.key == key)
    };
    for does in CapabilityDoesSpec::one_of_each() {
        let key = format!("{:?}", does.capability());
        assert!(declared(type_name::<EffectQueues>(), key), "{does:?}");
    }
    for kind in DeliveryKind::ALL {
        assert!(declared(type_name::<Deliverers>(), format!("{kind:?}")));
    }
    assert!(declared(type_name::<CastSpends>(), format!("{:?}", ())));
}
