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

use crate::actions::capability_does::CapabilityDoes;
use crate::actions::effect_queues::EffectQueues;
use crate::declare::book::Book;
use crate::declare::capability_set::CapabilitySet;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::players::player::Player;
use crate::release::Release;
use crate::release::test_match::TestMatch;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;
use crate::state_types::{Declared, StateTypes};
use crate::units::row_parts::RowParts;
use crate::units::unit_entity::UnitEntity;
use crate::units::view_column::ViewColumn;

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

/// A book as declared: its type, its owner, and the module that declares it.
#[derive(Debug)]
struct Declaring {
    ty: Named,
    owner: Option<Capability>,
    module: Option<Capability>,
}

/// Every declaration of the modules it visits.
#[derive(Debug, Default)]
struct DeclarationAudit {
    module: Option<Capability>,
    kinds: Vec<TypeId>,
    states: Vec<State>,
    resources: Vec<&'static str>,
    requires: Vec<(Named, Named)>,
    hosts: Vec<(Named, Option<Vec<TypeId>>)>,
    checks: Vec<Named>,
    books: Vec<Declaring>,
    columns: Vec<Named>,
    call_parts: Vec<Named>,
    player_parts: Vec<Named>,
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
    /// owner is the module that declares it; and a player's part is a state type of the player's
    /// kind.
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
        once(
            "player part",
            self.player_parts.iter().map(|ty| ty.name).collect(),
        );
        let kinds_declared = |kinds: &[TypeId]| kinds.iter().all(|kind| self.kinds.contains(kind));
        for state in &self.states {
            let hosted = self.hosts.iter().find(|(ty, _)| ty.id == state.ty.id);
            let kinds = state
                .kinds
                .as_ref()
                .or(hosted.and_then(|(_, kinds)| kinds.as_ref()));
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
            if book.owner != book.module {
                problems.push(format!("{} is declared by another module", book.ty.name));
            }
        }
        let player = [TypeId::of::<Player>()];
        for ty in &self.player_parts {
            let of_player = self
                .state_of(ty.id)
                .is_some_and(|state| state.kinds.as_deref() == Some(&player[..]));
            if !of_player {
                problems.push(format!("{} is no player's state", ty.name));
            }
        }
        problems
    }
}

impl StateTypes for DeclarationAudit {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
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

    fn derived<C: Component>(&mut self) {}

    fn resource<R: SimResource>(&mut self) {
        self.resources.push(R::NAME);
    }

    fn sim_component<C: Replication>(&mut self) {
        self.state::<C>();
    }
}

impl Declarations for DeclarationAudit {
    fn module(&mut self, module: Option<Capability>) {
        self.module = module;
    }

    fn kind<K: HostRule + Component>(&mut self) {
        self.kinds.push(TypeId::of::<K>());
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
        self.books.push(Declaring {
            ty: Named::of::<B>(),
            owner: B::OWNER,
            module: self.module,
        });
    }

    fn column<C: ViewColumn + Default, D: RowParts>(&mut self, _: ColumnFill<D, C>) {
        self.columns.push(Named::of::<C>());
    }

    fn call_part<P: CallPart + Default>(&mut self) {
        self.call_parts.push(Named::of::<P>());
    }

    fn player_part<C: SimComponent>(&mut self, _: fn(&World) -> C) {
        self.player_parts.push(Named::of::<C>());
    }

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}

#[test]
fn the_modules_declarations_keep_every_rule_between_them() {
    let mut audit = DeclarationAudit::default();
    Release::declare(Release::capabilities(), &mut audit);
    assert_eq!(audit.problems(), Vec::<String>::new());
    // The audit read every module: the core's four kinds, the stats' and the mode's parts of a
    // player, and the 18 books, 5 of the core's.
    assert_eq!(audit.kinds.len(), 4);
    assert_eq!(audit.player_parts.len(), 2);
    assert_eq!(audit.books.len(), 18);
    let core = audit.books.iter().filter(|book| book.owner.is_none());
    assert_eq!(core.count(), 5);
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

/// A book a test declares, as the stats' own.
#[derive(Resource, Debug, Default)]
struct StrayBook;

impl Book for StrayBook {
    const OWNER: Option<Capability> = Some(Capability::Stats);
    const SCRIPTS_ONLY: bool = false;
}

#[test]
fn each_rule_names_the_declaration_that_breaks_it() {
    // One module of the combat capability: a state type on no kind, declared twice, needed by a
    // unit though it is a part that sits nowhere, checked, and put on players; a book the stats
    // own; and a kind's part, a unit's, needing a type no module declares.
    let mut audit = DeclarationAudit::default();
    audit.module(None);
    audit.kind::<UnitEntity>();
    audit.module(Some(Capability::Combat));
    audit.component::<Stray>();
    audit.component::<Stray>();
    audit.require::<UnitEntity, Stray>();
    audit.require::<UnitEntity, Player>();
    audit.check::<Player>(|_| Ok(()));
    audit.book::<StrayBook>();
    audit.player_part(|_| Stray);
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
            format!("{stray} is no player's state"),
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

/// `capability` and each it builds on, those first.
fn with_needs(capability: Capability, into: &mut Vec<Capability>) {
    for &need in CapabilitySet::needs(capability) {
        with_needs(need, into);
    }
    if !into.contains(&capability) {
        into.push(capability);
    }
}

#[test]
fn each_listed_effect_queues_in_the_capability_it_names() {
    for does in CapabilityDoes::one_of_each() {
        let owner = does.capability();
        let mut declared = Vec::new();
        with_needs(owner, &mut declared);
        let sim = TestMatch::client(&declared);
        assert!(
            sim.world.resource::<EffectQueues>().holds(owner),
            "{owner:?}"
        );
    }
}
