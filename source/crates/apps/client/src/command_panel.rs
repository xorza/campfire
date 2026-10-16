use std::fmt::Write as _;
use std::mem;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::{Allow, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::picking::events::{Click, Pointer};
use bevy::picking::pointer::PointerButton;
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, FlexDirection, Node, PositionType, UiRect, Val};
use campfire_actions::{ActionBook, ActionSlots, ActionTarget, KindSpec};
use campfire_capabilities::{Action, Order};
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_net::PendingOrders;
use campfire_orders::OrdersAction;
use campfire_players::{PlayerResources, PlayerRows};
use campfire_production::{ProductionAction, Site, TrainQueue};
use campfire_sim::{EntityIndex, SimTick, StableId, Unpredicted};
use campfire_units::{Dead, Owner};
use campfire_values::{ActionId, ResourceId, UnitType};

use crate::action_names::ActionTexts;
use crate::own_player::OwnPlayer;
use crate::placing::Placing;
use crate::selection::{Selection, SelectionSystems};
use crate::ui_style::{self, BACK, DIMMED, TEXT};
use crate::view::unit_looks::UnitLooks;

/// The command panel (design 18, D7): it shows the first selected unit, in stable id order, of
/// the player's own that has a train or a build or is a site. Its rows: the player's amount of
/// each of the mode's resources; a button for each train and build of the unit's slots, with its
/// name and cost, dimmed while the player cannot pay it; the unit's train queue, each entry's
/// name and the head's progress, a click on an entry cancelling it; and of a site, its progress,
/// a click cancelling it. A train button orders the unit alone to train; a build button starts
/// the build preview.
#[derive(Debug, Clone)]
pub(crate) struct CommandPanel {
    pub(crate) resources: Vec<ResourceId>,
}

/// What the panel shows of the mode: the player resources whose amounts it shows, in the mode's
/// order.
#[derive(Resource, Debug)]
struct PanelSetup(CommandPanel);

/// The panel's root, shown while it shows a unit.
#[derive(Component, Debug)]
struct PanelRoot;

/// The panel's line of the player's amounts.
#[derive(Component, Debug)]
struct Funds;

/// The holder of the panel's rows.
#[derive(Component, Debug)]
struct Rows;

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

/// What a button's action makes: a unit the unit trains, or a building of a type it builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Making {
    Train,
    Build(UnitType),
}

/// What the panel reads of a unit: its slots, its train queue, and its site.
#[derive(Debug, Clone, Copy)]
struct PanelUnit<'a> {
    slots: Option<&'a ActionSlots>,
    queue: Option<&'a TrainQueue>,
    site: Option<&'a Site>,
}

/// The panel's root, the holder of its rows, and its line of amounts.
type PanelNodes<'w, 's> = (
    Single<'w, 's, &'static mut Visibility, With<PanelRoot>>,
    Single<'w, 's, Entity, With<Rows>>,
    Single<'w, 's, Entity, With<Funds>>,
);

/// The units the panel may show: each one's player, slots, train queue and site.
type PanelUnits<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Owner>,
        Option<&'static ActionSlots>,
        Option<&'static TrainQueue>,
        Option<&'static Site>,
    ),
    (Without<Dead>, Allow<Unpredicted>),
>;

impl Plugin for CommandPanel {
    fn build(&self, app: &mut App) {
        app.insert_resource(PanelSetup(self.clone()));
        app.init_resource::<Shown>();
        app.add_systems(Startup, Self::spawn);
        app.add_systems(Update, Self::show.after(SelectionSystems));
    }
}

impl CommandPanel {
    fn spawn(mut commands: Commands<'_, '_>) {
        let root = Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            bottom: Val::Px(12.0),
            width: Val::Px(320.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(4.0),
            ..Node::default()
        };
        commands
            .spawn((PanelRoot, root, BackgroundColor(BACK), Visibility::Hidden))
            .with_children(|panel| {
                panel.spawn((Funds, Text::default(), ui_style::font(), TextColor(TEXT)));
                let rows = Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..Node::default()
                };
                panel.spawn((Rows, rows));
            });
    }

    /// Shows the unit the panel reads: its rows, made again when what it shows changes, and
    /// each frame the player's amounts, the buttons' costs and dimming, and the progress of the
    /// queue's head and of a site.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn show(
        own: OwnPlayer<'_, '_>,
        (selection, index, units): (Res<'_, Selection>, Res<'_, EntityIndex>, PanelUnits<'_, '_>),
        (book, names, tick): (
            Res<'_, ActionBook>,
            Res<'_, ActionTexts>,
            Option<Res<'_, SimTick>>,
        ),
        (funds_rows, setup): (PlayerRows<'_, '_, PlayerResources>, Res<'_, PanelSetup>),
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
        let slot = own.slot();
        next.clear();
        for &unit in selection.get() {
            let Some(Ok((owner, slots, queue, site))) = index.get(unit).map(|at| units.get(at))
            else {
                continue;
            };
            if slot.is_none() || owner.map(|owner| owner.slot()) != slot {
                continue;
            }
            let held = PanelUnit { slots, queue, site };
            if next.fill(unit, held, &book) {
                break;
            }
        }
        if *shown != *next {
            mem::swap(&mut *shown, &mut *next);
            commands.entity(*rows).despawn_related::<Children>();
            shown.spawn_rows(&mut commands, *rows, &names);
        }
        let mut root = root.into_inner();
        let Some(unit) = shown.unit.and_then(|unit| index.get(unit)) else {
            root.set_if_neq(Visibility::Hidden);
            return;
        };
        root.set_if_neq(Visibility::Inherited);
        let Ok((_, slots, queue, site)) = units.get(unit) else {
            return;
        };
        let held = slot.and_then(|slot| funds_rows.of_slot(slot));
        line.clear();
        for (at, &resource) in setup.0.resources.iter().enumerate() {
            let amount = held.map_or(0, |held| held.amount(resource));
            let gap = if at == 0 { "" } else { "  " };
            write!(line, "{gap}{amount}").expect("a String takes any text");
        }
        ui_style::set_text(&mut texts, *funds, &line);
        for row in &buttons {
            let costs = slots
                .and_then(|slots| slots.slot(row.button.slot))
                .and_then(|held| Some(book.get(held.action?)?.resource_cost(held.rank?)));
            line.clear();
            for (at, cost) in costs.unwrap_or_default().iter().enumerate() {
                let gap = if at == 0 { "" } else { "  " };
                write!(line, "{gap}{}", cost.amount).expect("a String takes any text");
            }
            ui_style::set_text(&mut texts, row.cost, &line);
            let affords = costs
                .zip(held)
                .is_some_and(|(costs, held)| held.affords(costs));
            let color = if affords { TEXT } else { DIMMED };
            for line in [row.name, row.cost] {
                if let Ok(mut held) = colors.get_mut(line) {
                    held.set_if_neq(TextColor(color));
                }
            }
        }
        let now = tick.map(|tick| tick.start());
        for row in &queue_rows {
            line.clear();
            let head = (row.place == 0).then_some(queue).flatten();
            if let Some(percent) = head
                .zip(now)
                .and_then(|(queue, now)| head_percent(queue, now))
            {
                write!(line, "{percent}%").expect("a String takes any text");
            }
            ui_style::set_text(&mut texts, row.progress, &line);
        }
        for row in &site_rows {
            line.clear();
            let time = site.and_then(|site| book.get(site.action())?.windup_ticks(site.rank()));
            if let Some((site, time)) = site.zip(time) {
                let percent = site_percent(site.progress(), time);
                write!(line, "{percent}%").expect("a String takes any text");
            }
            ui_style::set_text(&mut texts, row.progress, &line);
        }
    }

    /// A row in `rows` of a line of `name` and a line of value, which the panel writes.
    fn spawn_row(commands: &mut Commands<'_, '_>, rows: Entity, name: &str) -> RowLines {
        let line = Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(12.0),
            ..Node::default()
        };
        let row = commands.spawn((line, ChildOf(rows))).id();
        let mut cell = |text: Text| {
            commands
                .spawn((text, ui_style::font(), TextColor(TEXT), ChildOf(row)))
                .id()
        };
        let name = cell(Text::new(name));
        let value = cell(Text::default());
        RowLines { row, name, value }
    }

    /// A left click on a button's row trains with its slot, or starts the build preview of its
    /// build at its building's angle.
    fn press(
        click: On<'_, '_, Pointer<Click>>,
        rows: Query<'_, '_, &ButtonRow>,
        shown: Res<'_, Shown>,
        looks: Option<Res<'_, UnitLooks>>,
        mut orders: ResMut<'_, PendingOrders>,
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
            Making::Train => {
                let target = ActionTarget::None;
                orders.push(Order::one(unit, OrdersAction::Slot { slot, target }));
            }
            Making::Build(building) => {
                let angle = looks
                    .as_deref()
                    .map_or(Num::ZERO, |looks| looks.placement_angle(building));
                commands.insert_resource(Placing {
                    builder: unit,
                    slot,
                    action,
                    building,
                    angle,
                });
            }
        }
    }

    /// A left click on a row of the queue cancels its entry.
    fn cancel_train(
        click: On<'_, '_, Pointer<Click>>,
        rows: Query<'_, '_, &QueueRow>,
        shown: Res<'_, Shown>,
        orders: ResMut<'_, PendingOrders>,
    ) {
        if let Ok(row) = rows.get(click.entity) {
            let place = row.place;
            Self::cancel(
                &click,
                &shown,
                orders,
                ProductionAction::CancelTrain { place },
            );
        }
    }

    /// A left click on a site's row cancels the site.
    fn cancel_build(
        click: On<'_, '_, Pointer<Click>>,
        shown: Res<'_, Shown>,
        orders: ResMut<'_, PendingOrders>,
    ) {
        Self::cancel(&click, &shown, orders, ProductionAction::CancelBuild);
    }

    /// Orders the shown unit to `action`, for a left click.
    fn cancel(
        click: &Pointer<Click>,
        shown: &Shown,
        mut orders: ResMut<'_, PendingOrders>,
        action: ProductionAction,
    ) {
        if click.button != PointerButton::Primary {
            return;
        }
        if let Some(unit) = shown.unit {
            orders.push(Order::one(unit, Action::from(action)));
        }
    }
}

/// The entities of a row: the row, and its lines of name and of value.
#[derive(Debug, Clone, Copy)]
struct RowLines {
    row: Entity,
    name: Entity,
    value: Entity,
}

impl Shown {
    /// Shows nothing.
    fn clear(&mut self) {
        self.unit = None;
        self.buttons.clear();
        self.queue.clear();
        self.site = None;
    }

    /// Shows `unit`, as `held` and `book` give it, unless it is no site and has no button: then
    /// it shows nothing. Whether it shows the unit. A site shows its own row alone, as its actions
    /// wait for it to complete.
    fn fill(&mut self, unit: StableId, held: PanelUnit<'_>, book: &ActionBook) -> bool {
        self.clear();
        if let Some(site) = held.site {
            self.unit = Some(unit);
            self.site = Some(site.action());
            return true;
        }
        let slots = held.slots.into_iter().flat_map(ActionSlots::indexed);
        self.buttons.extend(slots.filter_map(|(slot, held)| {
            let action = held.action?;
            let making = match book.get(action)?.kind {
                KindSpec::Train(_) => Making::Train,
                KindSpec::Build(building) => Making::Build(building),
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
    fn spawn_rows(&self, commands: &mut Commands<'_, '_>, rows: Entity, names: &ActionTexts) {
        for &button in &self.buttons {
            let lines = CommandPanel::spawn_row(commands, rows, names.of(button.action));
            commands
                .entity(lines.row)
                .insert(ButtonRow {
                    button,
                    name: lines.name,
                    cost: lines.value,
                })
                .observe(CommandPanel::press);
        }
        for (place, &action) in (0..=u8::MAX).zip(&self.queue) {
            let lines = CommandPanel::spawn_row(commands, rows, names.of(action));
            commands
                .entity(lines.row)
                .insert(QueueRow {
                    place,
                    progress: lines.value,
                })
                .observe(CommandPanel::cancel_train);
        }
        if let Some(action) = self.site {
            let lines = CommandPanel::spawn_row(commands, rows, names.of(action));
            commands
                .entity(lines.row)
                .insert(SiteRow {
                    progress: lines.value,
                })
                .observe(CommandPanel::cancel_build);
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

/// The share of `time` that `progress` is, in whole percent rounded down; 100 for a build of no
/// time.
fn site_percent(progress: Num, time: Num) -> i64 {
    let (progress, time) = (i128::from(progress.to_bits()), i128::from(time.to_bits()));
    if time <= 0 {
        return 100;
    }
    i64::try_from(progress * 100 / time).expect("a site's progress is at most its time")
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::{IntoSystem, System};
    use bevy::ecs::world::World;
    use campfire_actions::{Actions, SlotKind};
    use campfire_capabilities::Match;
    use campfire_production::Queued;
    use campfire_sim::{Capability, IdAllocator};
    use campfire_values::Rank;

    use super::*;

    #[test]
    fn the_panel_takes_no_conflicting_access() {
        // Bevy checks that a system's queries do not conflict as it initializes the system.
        let mut world = World::new();
        let mut panel = IntoSystem::into_system(CommandPanel::show);
        panel.initialize(&mut world);
    }

    #[test]
    fn the_panel_shows_a_producers_trains_and_builds_its_queue_and_a_site() {
        let mut world = Match::predicting(&[Capability::Production]);
        let (ranger, barracks) = (UnitType::new(1), UnitType::new(2));
        let time = Ticks::new(150);
        let train = Actions::add_train(&mut world, ranger, time, None);
        let build = Actions::add_build(&mut world, barracks, 10, Ticks::new(300), None);
        let book = world.resource::<ActionBook>();
        let kind = SlotKind::new(0);
        let rank = Some(Rank::FIRST);
        // A dozer's slots: a build in slot 0 and a train in slot 1.
        let slots = ActionSlots::new([(build, kind, rank), (train, kind, rank)]);
        let mut queue = TrainQueue::default();
        let queued = Queued {
            action: train,
            rank: Rank::FIRST,
            time,
            paid: 0,
        };
        queue.push(queued, &[], Tick::new(0));
        queue.push(queued, &[], Tick::new(0));
        let mut ids = IdAllocator::default();
        let (center, wall) = (ids.allocate(), ids.allocate());
        let mut shown = Shown::default();
        let held = PanelUnit {
            slots: Some(&slots),
            queue: Some(&queue),
            site: None,
        };
        assert!(shown.fill(center, held, book));
        assert_eq!(
            shown,
            Shown {
                unit: Some(center),
                buttons: vec![
                    Button {
                        slot: 0,
                        action: build,
                        making: Making::Build(barracks),
                    },
                    Button {
                        slot: 1,
                        action: train,
                        making: Making::Train,
                    },
                ],
                queue: vec![train, train],
                site: None,
            }
        );
        // A unit of no train or build shows nothing.
        let none = ActionSlots::new([]);
        let held = PanelUnit {
            slots: Some(&none),
            queue: None,
            site: None,
        };
        assert!(!shown.fill(wall, held, book));
        assert_eq!(shown, Shown::default());
        // A site shows its row alone, though its type trains.
        let site = Site::new(build, Rank::FIRST, Num::ZERO, &[], None);
        let held = PanelUnit {
            slots: Some(&slots),
            queue: Some(&queue),
            site: Some(&site),
        };
        assert!(shown.fill(wall, held, book));
        assert_eq!(
            shown,
            Shown {
                unit: Some(wall),
                site: Some(build),
                ..Shown::default()
            }
        );
    }

    #[test]
    fn progress_shows_in_whole_percent_rounded_down() {
        // A head of 150 ticks from tick 0, done at 150: at 60, 60 / 150 = 40 %; at 149, 99.3 %
        // shows 99; at 150 and past, 100.
        let train = ActionId::new(0);
        let queued = Queued {
            action: train,
            rank: Rank::FIRST,
            time: Ticks::new(150),
            paid: 0,
        };
        let mut queue = TrainQueue::default();
        assert_eq!(head_percent(&queue, Tick::new(0)), None);
        queue.push(queued, &[], Tick::new(0));
        let at = |now| head_percent(&queue, Tick::new(now));
        assert_eq!(
            [at(0), at(60), at(149), at(150), at(400)],
            [0, 40, 99, 100, 100].map(Some)
        );
        // A head that reached the head at 100, after another: at 160, 60 of its 150 ticks.
        let mut queue = TrainQueue::default();
        queue.push(queued, &[], Tick::new(100));
        assert_eq!(head_percent(&queue, Tick::new(160)), Some(40));
        // A site 0.7 of the way to 300 ticks: 210 / 300 = 70 % exactly; one 2⁻³² below it, 69.
        let time = Num::int(300);
        let progress = Num::int(210);
        assert_eq!(site_percent(progress, time), 70);
        assert_eq!(
            site_percent(Num::from_bits(progress.to_bits() - 1), time),
            69
        );
        assert_eq!(site_percent(Num::ZERO, time), 0);
        assert_eq!(site_percent(time, time), 100);
        assert_eq!(site_percent(Num::ZERO, Num::ZERO), 100);
    }
}
