use std::fmt::Write as _;
use std::mem;
use std::num::NonZeroU64;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Allow, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::state::state::NextState;
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use campfire_actions::{ActionBook, ActionKind, ActionSlots, ActionTarget};
use campfire_capabilities::{Directive, Order};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ById;
use campfire_math::Num;
use campfire_net::PendingOrders;
use campfire_orders::OrdersDirective;
use campfire_players::ResourceAmount;
use campfire_production::{
    BuildBook, ProductionDirective, ResearchHolder, ResearchSpec, Site, SiteProgress, TrainQueue,
};
use campfire_script::MatchNames;
use campfire_sim::{EntityIndex, MatchRate, SimTick, StableId, Unpredicted};
use campfire_stats::Modifiers;
use campfire_units::{Dead, Owner};
use campfire_values::{ActionId, ModifierId, NameId, ResourceId, UnitType};

use crate::input_mode::InputMode;
use crate::name_texts::NameTexts;
use crate::own_player::OwnPlayer;
use crate::placing::Placing;
use crate::selection::{Selection, SelectionSystems};
use crate::ui_style::{self, Corner, DIMMED, Panel, PanelFunds, PanelRow, PanelRows, TEXT};
use crate::view::unit_looks::UnitLooks;

/// The command panel (02-engine-core.md, A command panel from the actions; 02-engine-core.md, The
/// panel shows a research as an action): it shows the first selected unit, in stable id order, of
/// the player's own that has a train, a build or a research or is a site. Its rows: the player's
/// amount of each of the mode's resources; a button for each train, build and research of the
/// unit's slots, with its name, its cost and its time, dimmed while the player cannot pay it, a
/// research its player or its unit holds as done; the unit's train queue, each entry's name and the
/// head's progress, a click on an entry cancelling it; and of a site, its progress, a click
/// cancelling it. A train or a research button orders the unit alone to start it; a build button
/// starts the build preview.
#[derive(Debug)]
pub(crate) struct CommandPanelPlugin;

/// What the panel shows of the mode: the player resources whose amounts it shows, in the order
/// the match names them.
#[derive(Resource, Debug)]
struct PanelSetup {
    resources: Vec<ResourceId>,
}

/// A button's row: its button, and its line of cost.
#[derive(Component, Debug)]
struct ButtonRow {
    button: Button,
    name: Entity,
    cost: Entity,
}

/// A row of the queue: its entry's place, and its line of progress.
#[derive(Component, Debug)]
struct QueueRow {
    place: u8,
    progress: Entity,
}

/// A site's row: its line of progress.
#[derive(Component, Debug)]
struct SiteRow {
    progress: Entity,
}

/// What the panel shows, whose rows it holds: the unit, none before it shows one, its buttons
/// in slot order, its queue's actions in order, and the action that placed it, of a site.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
struct Shown {
    unit: Option<StableId>,
    buttons: Vec<Button>,
    queue: Vec<ActionId>,
    site: Option<ActionId>,
}

/// A button of the panel: the slot it orders, its action, and what the action makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Button {
    slot: u8,
    action: ActionId,
    making: Making,
}

/// What a button's action makes: a unit the unit trains, a building of a type it builds, or a
/// modifier it researches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Making {
    Train,
    Build(UnitType),
    Research(ResearchSpec),
}

/// The books the panel tells a train, a build and a research apart by, a build's building, and a
/// research's modifier, none in a mode of no research.
#[derive(Debug, Clone, Copy)]
struct MakingBooks<'a> {
    actions: &'a ActionBook,
    builds: &'a BuildBook,
    researches: Option<&'a ById<ActionId, ResearchSpec>>,
}

/// What the panel reads of a unit: its slots, its train queue, and its site.
#[derive(Debug, Clone, Copy)]
struct PanelUnit<'a> {
    slots: Option<&'a ActionSlots>,
    queue: Option<&'a TrainQueue>,
    site: Option<&'a Site>,
}

/// What a button's line of value shows: its action's cost and its time in whole seconds, rounded
/// up; that its research is held; or nothing, for an action its unit has not learned.
#[derive(Debug, Clone, Copy)]
enum ButtonValue<'a> {
    Costs {
        costs: &'a [ResourceAmount],
        seconds: u64,
    },
    Done,
    Unlearned,
}

/// The panel's root, the holder of its rows, and its line of amounts.
type PanelNodes<'w, 's> = (
    Single<'w, 's, &'static mut Visibility, With<Panel<CommandPanelPlugin>>>,
    Single<'w, 's, Entity, With<PanelRows<CommandPanelPlugin>>>,
    Single<'w, 's, Entity, With<PanelFunds<CommandPanelPlugin>>>,
);

/// The books the panel reads: the actions, the builds, the researches, none in a mode of no
/// research, and the actions' names.
type PanelBooks<'w> = (
    Res<'w, ActionBook>,
    Res<'w, BuildBook>,
    Option<Res<'w, ById<ActionId, ResearchSpec>>>,
    Res<'w, NameTexts<ActionId>>,
);

/// The units the panel may show: each one's player, slots, train queue, site and its progress,
/// and the modifiers it carries.
type PanelUnits<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Owner>,
        Option<&'static ActionSlots>,
        Option<&'static TrainQueue>,
        Option<&'static Site>,
        Option<&'static SiteProgress>,
        Option<&'static Modifiers>,
    ),
    (Without<Dead>, Allow<Unpredicted>),
>;

impl Plugin for CommandPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shown>();
        app.add_systems(Startup, (Self::set_up, Self::spawn));
        app.add_systems(Update, Self::show.after(SelectionSystems));
    }
}

impl CommandPanelPlugin {
    /// Takes the player resources the match names, whose amounts the panel shows.
    fn set_up(names: Res<'_, MatchNames>, mut commands: Commands<'_, '_>) {
        let resources = (0..names.count::<ResourceId>())
            .map(ResourceId::at)
            .collect();
        commands.insert_resource(PanelSetup { resources });
    }

    fn spawn(mut commands: Commands<'_, '_>) {
        Panel::<Self>::spawn(&mut commands, Corner::BottomRight, 320.0);
    }

    /// Shows the unit the panel reads: its rows, made again when what it shows changes, and
    /// each frame the player's amounts, the buttons' costs, times, dimming and held researches,
    /// and the progress of the queue's head and of a site.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn show(
        own: OwnPlayer<'_, '_>,
        (selection, index, units): (Res<'_, Selection>, Res<'_, EntityIndex>, PanelUnits<'_, '_>),
        (book, builds, researches, names): PanelBooks<'_>,
        (tick, rate): (Res<'_, SimTick>, Res<'_, MatchRate>),
        setup: Res<'_, PanelSetup>,
        (mut shown, mut next): (ResMut<'_, Shown>, Local<'_, Shown>),
        (root, rows, funds): PanelNodes<'_, '_>,
        (mut texts, mut colors): (Query<'_, '_, &mut Text>, Query<'_, '_, &mut TextColor>),
        (buttons, queue_rows, site_rows): (
            Query<'_, '_, &ButtonRow>,
            Query<'_, '_, &QueueRow>,
            Query<'_, '_, &SiteRow>,
        ),
        mut commands: Commands<'_, '_>,
        mut line: Local<'_, String>,
    ) {
        let player_slot = own.player_slot();
        next.clear();
        for &unit in selection.get() {
            let Some(Ok((owner, slots, queue, site, ..))) = index.get(unit).map(|at| units.get(at))
            else {
                continue;
            };
            if player_slot.is_none() || owner.map(|owner| owner.player_slot()) != player_slot {
                continue;
            }
            let held = PanelUnit { slots, queue, site };
            let books = MakingBooks {
                actions: &book,
                builds: &builds,
                researches: researches.as_deref(),
            };
            if next.fill(unit, held, books) {
                break;
            }
        }
        if *shown != *next {
            mem::swap(&mut *shown, &mut *next);
            commands.entity(*rows).despawn_related::<Children>();
            shown.spawn_rows(&mut commands, *rows, &names);
        }
        let mut root = root.into_inner();
        let Some((id, unit)) = shown.unit.and_then(|id| Some((id, index.get(id)?))) else {
            root.set_if_neq(Visibility::Hidden);
            return;
        };
        root.set_if_neq(Visibility::Inherited);
        let Ok((_, slots, queue, site, progress, carried)) = units.get(unit) else {
            return;
        };
        line.clear();
        for (at, &resource) in setup.resources.iter().enumerate() {
            let amount = own.amount(resource);
            let gap = if at == 0 { "" } else { "  " };
            write!(line, "{gap}{amount}").expect("a String takes any text");
        }
        ui_style::set_text(&mut texts, *funds, &line);
        let hz = NonZeroU64::from(rate.hz()).get();
        for row in &buttons {
            let held = slots.and_then(|slots| slots.slot(row.button.slot));
            let ranked = held.and_then(|held| Some((book.get(held.action?)?, held.rank?)));
            let done = row.button.making.done(
                id,
                |modifier| own.holds(modifier),
                |modifier, source| {
                    carried.is_some_and(|carried| carried.get(modifier, Some(source)).is_some())
                },
            );
            let value = match ranked {
                _ if done => ButtonValue::Done,
                Some((action, rank)) => ButtonValue::Costs {
                    costs: action.resource_cost(rank),
                    seconds: action.values(rank).windup.get().div_ceil(hz),
                },
                None => ButtonValue::Unlearned,
            };
            line.clear();
            value.write(&mut line);
            ui_style::set_text(&mut texts, row.cost, &line);
            let affords = match value {
                ButtonValue::Costs { costs, .. } => own.affords(costs),
                ButtonValue::Done | ButtonValue::Unlearned => false,
            };
            let color = if affords { TEXT } else { DIMMED };
            for line in [row.name, row.cost] {
                if let Ok(mut held) = colors.get_mut(line) {
                    held.set_if_neq(TextColor(color));
                }
            }
        }
        let now = tick.start();
        for row in &queue_rows {
            line.clear();
            let head = (row.place == 0).then_some(queue).flatten();
            if let Some(percent) = head.and_then(|queue| head_percent(queue, now)) {
                write!(line, "{percent}%").expect("a String takes any text");
            }
            ui_style::set_text(&mut texts, row.progress, &line);
        }
        for row in &site_rows {
            line.clear();
            if let Some((progress, time)) = progress.zip(site.map(Site::time)) {
                let percent = site_percent(progress.get(), time);
                write!(line, "{percent}%").expect("a String takes any text");
            }
            ui_style::set_text(&mut texts, row.progress, &line);
        }
    }

    /// A left click on a button's row trains with its slot, or starts the build preview of its
    /// build at its building's angle.
    fn press(
        click: On<'_, '_, PointerClick>,
        rows: Query<'_, '_, &ButtonRow>,
        shown: Res<'_, Shown>,
        looks: Res<'_, UnitLooks>,
        (mut orders, mut mode): (ResMut<'_, PendingOrders>, ResMut<'_, NextState<InputMode>>),
        mut commands: Commands<'_, '_>,
    ) {
        if click.button != PointerButton::Primary {
            return;
        }
        let (Ok(row), Some(unit)) = (rows.get(click.entity), shown.unit) else {
            return;
        };
        let Button {
            slot,
            action,
            making,
        } = row.button;
        match making {
            Making::Train | Making::Research(_) => {
                let target = ActionTarget::None;
                orders.push(Order::one(unit, OrdersDirective::Slot { slot, target }));
            }
            Making::Build(building) => {
                let angle = looks.placement_angle(building);
                commands.insert_resource(Placing {
                    builder: unit,
                    slot,
                    action,
                    building,
                    angle,
                });
                mode.set_if_different(InputMode::Place);
            }
        }
    }

    /// A left click on a row of the queue cancels its entry.
    fn cancel_train(
        click: On<'_, '_, PointerClick>,
        rows: Query<'_, '_, &QueueRow>,
        shown: Res<'_, Shown>,
        orders: ResMut<'_, PendingOrders>,
    ) {
        if let Ok(row) = rows.get(click.entity) {
            let place = row.place;
            shown.order(&click, orders, ProductionDirective::CancelTrain { place });
        }
    }

    /// A left click on a site's row cancels the site.
    fn cancel_build(
        click: On<'_, '_, PointerClick>,
        shown: Res<'_, Shown>,
        orders: ResMut<'_, PendingOrders>,
    ) {
        shown.order(&click, orders, ProductionDirective::CancelBuild);
    }
}

impl Shown {
    /// Orders the shown unit to `directive`, for a left click.
    fn order(
        &self,
        click: &PointerClick,
        mut orders: ResMut<'_, PendingOrders>,
        directive: ProductionDirective,
    ) {
        if click.button != PointerButton::Primary {
            return;
        }
        if let Some(unit) = self.unit {
            orders.push(Order::one(unit, Directive::from(directive)));
        }
    }

    /// Shows nothing.
    fn clear(&mut self) {
        self.unit = None;
        self.buttons.clear();
        self.queue.clear();
        self.site = None;
    }

    /// Shows `unit`, as `held` and `books` give it, unless it is no site and has no button: then
    /// it shows nothing. Whether it shows the unit. A site shows its own row alone, as its actions
    /// wait for it to complete.
    fn fill(&mut self, unit: StableId, held: PanelUnit<'_>, books: MakingBooks<'_>) -> bool {
        self.clear();
        if let Some(site) = held.site {
            self.unit = Some(unit);
            self.site = Some(site.action());
            return true;
        }
        let slots = held.slots.into_iter().flat_map(ActionSlots::indexed);
        self.buttons.extend(slots.filter_map(|(slot, held)| {
            let action = held.action?;
            let making = match books.actions.get(action)?.kind {
                ActionKind::Train => Making::Train,
                ActionKind::Build => Making::Build(books.builds.of(action)?.unit_type),
                ActionKind::Research => Making::Research(*books.researches?.get(action)?),
                _ => return None,
            };
            Some(Button {
                slot,
                action,
                making,
            })
        }));
        if self.buttons.is_empty() {
            return false;
        }
        self.unit = Some(unit);
        let queue = held.queue.map_or(&[][..], TrainQueue::entries);
        self.queue.extend(queue.iter().map(|queued| queued.action));
        true
    }

    /// Spawns its rows in `rows`, each named as `names` names its action: its buttons, then its
    /// queue's entries, then its site.
    fn spawn_rows(
        &self,
        commands: &mut Commands<'_, '_>,
        rows: Entity,
        names: &NameTexts<ActionId>,
    ) {
        for &button in &self.buttons {
            let lines = PanelRow::spawn(commands, rows, names.of(button.action));
            commands
                .entity(lines.row)
                .insert(ButtonRow {
                    button,
                    name: lines.name,
                    cost: lines.value,
                })
                .observe(CommandPanelPlugin::press);
        }
        for (place, &action) in (0..=u8::MAX).zip(&self.queue) {
            let lines = PanelRow::spawn(commands, rows, names.of(action));
            commands
                .entity(lines.row)
                .insert(QueueRow {
                    place,
                    progress: lines.value,
                })
                .observe(CommandPanelPlugin::cancel_train);
        }
        if let Some(action) = self.site {
            let lines = PanelRow::spawn(commands, rows, names.of(action));
            commands
                .entity(lines.row)
                .insert(SiteRow {
                    progress: lines.value,
                })
                .observe(CommandPanelPlugin::cancel_build);
        }
    }
}

impl Making {
    /// Whether it is a research held already, as its player or its unit, `unit`, holds it: a
    /// player's research by the player, as `player_holds` says of a modifier; a producer's by the
    /// unit from itself, as `unit_holds` says of a modifier and its source
    /// (04-capabilities/production.md, An upgrade is a modifier).
    fn done(
        self,
        unit: StableId,
        player_holds: impl Fn(ModifierId) -> bool,
        unit_holds: impl Fn(ModifierId, StableId) -> bool,
    ) -> bool {
        let Self::Research(spec) = self else {
            return false;
        };
        match spec.holder {
            ResearchHolder::Player => player_holds(spec.modifier),
            ResearchHolder::Producer => unit_holds(spec.modifier, unit),
        }
    }
}

impl ButtonValue<'_> {
    /// Writes it into `line`: each cost's amount, then the seconds, `N s`; `done`; or nothing.
    fn write(self, line: &mut String) {
        match self {
            Self::Costs { costs, seconds } => {
                for cost in costs {
                    write!(line, "{}  ", cost.amount).expect("a String takes any text");
                }
                write!(line, "{seconds} s").expect("a String takes any text");
            }
            Self::Done => line.push_str("done"),
            Self::Unlearned => {}
        }
    }
}

/// The share of its time the head of `queue` ran by `now`, in whole percent rounded down, so
/// 100 shows only once its time ends; none for an empty queue.
fn head_percent(queue: &TrainQueue, now: Tick) -> Option<u64> {
    let head = queue.entries().first()?;
    let done = queue.head_done()?;
    let time = head.time.get();
    let left = done.since(now).map_or(0, Ticks::get).min(time);
    Some(match time {
        0 => 100,
        _ => (time - left) * 100 / time,
    })
}

/// The share of `time` that `progress` is, in whole percent rounded down, from 0 to 100; 100 for
/// a build of no time. A site's progress is at most its time, as the sim's check of it holds.
fn site_percent(progress: Num, time: Num) -> i64 {
    debug_assert!(
        Num::ZERO <= progress && progress <= time.max(Num::ZERO),
        "a site's progress is from 0 to its time"
    );
    let (progress, time) = (i128::from(progress.to_bits()), i128::from(time.to_bits()));
    if time <= 0 {
        return 100;
    }
    i64::try_from(progress * 100 / time).expect("a share from 0 to 100 fits")
}

#[cfg(test)]
mod tests;
