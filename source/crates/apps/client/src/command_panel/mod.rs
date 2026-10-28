use std::fmt::{self, Write as _};
use std::mem;
use std::num::{NonZeroU8, NonZeroU64};

use bevy::app::{App, Plugin, PreUpdate, Startup, Update};
use bevy::asset::{AssetPath, AssetServer};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::message::MessageWriter;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Allow, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single, SystemParam};
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::input::{ButtonInput, InputSystems};
use bevy::picking::Pickable;
use bevy::picking::events::{PointerClick, PointerOut, PointerOver};
use bevy::picking::pointer::PointerButton;
use bevy::state::condition::in_state;
use bevy::state::state::{NextState, OnExit};
use bevy::text::{TextColor, TextFont};
use bevy::ui::widget::{ImageNode, Text};
use bevy::ui::{
    AlignItems, BackgroundColor, Display, FlexDirection, GridPlacement, GridTrackRepetition,
    JustifyContent, Node, PositionType, RepeatedGridTrack, UiRect, Val,
};
use campfire_actions::{
    ActionBook, ActionKind, ActionSlots, ActionTarget, PlayerCooldowns, SlotTimes,
};
use campfire_capabilities::Order;
use campfire_client_files::{Chord, GridCell, Key, PanelCommand};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ById;
use campfire_interaction::Riders;
use campfire_math::Num;
use campfire_net::PendingOrders;
use campfire_orders::OrdersDirective;
use campfire_players::{PlayerRows, ResourceAmount};
use campfire_production::{
    BuildBook, HeldPlaces, ProductionDirective, RequirementBook, ResearchHolder, ResearchSpec,
    SellSpec, Site, SiteProgress, TrainQueue, TrainSpec,
};
use campfire_script::{Filter, MatchNames};
use campfire_sim::{EntityIndex, MatchRate, SimTick, StableId, Unpredicted};
use campfire_stats::Modifiers;
use campfire_units::{Dead, Owner, TypeOriginBook};
use campfire_values::{ActionId, ModifierId, NameId, ResourceId, TagSet, UnitTags, UnitType};

use crate::command_panel::aimed_command::AimedCommand;
use crate::command_panel::panel_looks::{CellFace, LookSources, PanelLooks, PlacedButton};
use crate::engine_text::EngineText;
use crate::input_map::{Act, InputMap};
use crate::input_mode::InputMode;
use crate::name_texts::NameTexts;
use crate::orders::OrderSystems;
use crate::own_player::OwnPlayer;
use crate::placing::Placing;
use crate::pointer::Pointer;
use crate::selection::{Selection, SelectionSystems, SelectionVoice};
use crate::ui_style::{self, Corner, DIMMED, Panel, PanelFunds, PanelRow, PanelRows, TEXT};
use crate::view::client_data::ClientLoad;
use crate::view::float_num;
use crate::view::match_hud::MatchHud;
use crate::view::package_source::PackageAssets;
use crate::view::unit_looks::UnitLooks;

mod aimed_command;
mod panel_looks;

/// The command panel (02-engine-core.md, A command panel from the actions; 02-engine-core.md, A key
/// presses a shown button; 02-engine-core.md, A tooltip tells what an icon hides;
/// 02-engine-core.md, The panel's commands are the engine's orders): it shows the first selected
/// unit, in stable id order, of the player's own that has a button or is a site. It shows the
/// player's amount of each of the mode's resources; the unit's buttons in the HUD file's grid, at
/// the places its look gives, or a look of none its trains, builds and researches in slot order,
/// each its icon or its name, dimmed while the player cannot press it, as one it cannot pay or a
/// research its player or its unit holds; a container's riders, each at an exit place, its icon its
/// type's; the unit's train queue, each entry's icon or name, the head's progress a bar over it, a
/// click on an entry cancelling it; of a site, its progress, a click cancelling it; and the name,
/// the key, the cost and the time of the button under the cursor. A left click on a button, or a
/// key with no modifier that a shown button's look gives, presses it: a train or a research orders
/// the unit alone to start it; a build starts the build preview; a sale, a stop and an evacuation
/// order the selection at once; a rider's button orders its container to let it out
/// (02-engine-core.md, The client orders entries by the right click); an attack move and a rally
/// take the point the next left click on the ground picks, a right click or the drop chord dropping
/// it.
#[derive(Debug)]
pub(crate) struct CommandPanelPlugin;

/// What the panel shows of the mode: the player resources whose amounts it shows, in the order
/// the match names them.
#[derive(Resource, Debug)]
struct PanelSetup {
    resources: Vec<ResourceId>,
}

/// A button's cell of the grid: its button's place among the shown buttons, and the node of its
/// face, its icon or its name.
#[derive(Component, Debug)]
struct ButtonCell {
    at: usize,
    face: Entity,
}

/// Whether the player may press each shown button now, in the shown buttons' order, written each
/// frame the panel shows, so a press reads it in the frame after the panel's cells change, before
/// they show; none while the panel shows nothing.
#[derive(Resource, Debug, Default)]
struct Pressable(Vec<bool>);

/// A cell of the queue: its entry's place, and the bar of its progress, the head's alone.
#[derive(Component, Debug)]
struct QueueCell {
    place: u8,
    bar: Option<Entity>,
}

/// A site's row: its line of progress.
#[derive(Component, Debug)]
struct SiteRow {
    progress: Entity,
}

/// The line of the name, the key, the cost and the time of the button under the cursor, which
/// stands over the panel, out of its layout, so its text moves no button out from under the
/// cursor; hidden while it is empty.
#[derive(Component, Debug)]
struct Tooltip;

/// The cell of the button under the cursor, none off every button.
#[derive(Resource, Debug, Default)]
struct Hovered(Option<Entity>);

/// What the panel shows, whose cells it holds: the unit, none before it shows one, its buttons in
/// the order of their places, its queue's actions in order, and the action that placed it, of a
/// site.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
struct Shown {
    unit: Option<StableId>,
    buttons: Vec<Button>,
    queue: Vec<ActionId>,
    site: Option<ActionId>,
}

/// A button of the panel: its cell of the grid, what it does, and its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Button {
    cell: GridCell,
    does: Does,
    key: Option<Key>,
}

/// What a button does: start an action of its unit's slot, order a command, or let a rider of
/// its unit, a container, out, the button showing the rider's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Does {
    Action {
        slot: u8,
        action: ActionId,
        making: Making,
    },
    Command(PanelCommand),
    Exit {
        rider: StableId,
        unit_type: UnitType,
    },
}

/// What a button's action makes: a unit the unit trains, a building of a type it builds, or a
/// modifier it researches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Making {
    Train,
    Build(UnitType),
    Research(ResearchSpec),
    /// A cast aimed at a point of the ground.
    Cast,
}

/// The books the panel tells a train, a build and a research apart by, a build's building, and a
/// research's modifier, none in a mode of no research; and the sales, none in a mode of none.
#[derive(Debug, Clone, Copy)]
struct MakingBooks<'a> {
    actions: &'a ActionBook,
    builds: &'a BuildBook,
    researches: Option<&'a ById<ActionId, ResearchSpec>>,
    sales: Option<&'a ById<UnitType, SellSpec>>,
}

/// What the panel reads of a unit: its type, its slots, its train queue, its site, its tags, and
/// its riders' ids and types, in the order they entered, of a container.
#[derive(Debug, Clone, Copy)]
struct PanelUnit<'a> {
    unit_type: UnitType,
    slots: Option<&'a ActionSlots>,
    queue: Option<&'a TrainQueue>,
    site: Option<&'a Site>,
    tags: TagSet,
    riders: &'a [(StableId, UnitType)],
}

/// What a button's cost shows: its action's cost and its time in whole seconds, rounded up; that
/// its research is held; or nothing, for an action its unit has not learned and a command.
#[derive(Debug, Clone, Copy)]
enum ButtonValue<'a> {
    Costs {
        costs: &'a [ResourceAmount],
        seconds: u64,
    },
    /// A cast on its cooldown, ready in whole seconds, rounded up.
    Cooling {
        seconds: u64,
    },
    Done,
    Nothing,
}

/// What a press of a button does: send an order, place a build, or aim a command.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pressed {
    Order(Order),
    Place(Placing),
    Aim(AimedCommand),
}

/// A button's tooltip: its name, its key, and its cost.
#[derive(Debug, Clone, Copy)]
struct ButtonTip<'a> {
    name: &'a str,
    key: Option<Key>,
    value: ButtonValue<'a>,
}

/// The panel's root, the holder of its rows, and its line of amounts.
type PanelNodes<'w, 's> = (
    Single<'w, 's, &'static mut Visibility, With<Panel<CommandPanelPlugin>>>,
    Single<'w, 's, Entity, With<PanelRows<CommandPanelPlugin>>>,
    Single<'w, 's, Entity, With<PanelFunds<CommandPanelPlugin>>>,
);

/// The books the panel reads: the actions, the builds, the researches, none in a mode of no
/// research, the sales, none in a mode of none, and the actions' names.
type PanelBooks<'w> = (
    Res<'w, ActionBook>,
    Res<'w, BuildBook>,
    Option<Res<'w, ById<ActionId, ResearchSpec>>>,
    Option<Res<'w, ById<UnitType, SellSpec>>>,
    Res<'w, NameTexts<ActionId>>,
    Option<Res<'w, ById<ActionId, TrainSpec>>>,
);

/// The units the panel may show: each one's player, type, slots, train queue, site and its
/// progress, the modifiers it carries, its slots' times, its riders and its places.
type PanelUnits<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Owner>,
        &'static UnitType,
        Option<&'static ActionSlots>,
        Option<&'static TrainQueue>,
        Option<&'static Site>,
        Option<&'static SiteProgress>,
        Option<&'static Modifiers>,
        Option<&'static UnitTags>,
        Option<&'static SlotTimes>,
        Option<&'static Riders>,
        Option<&'static HeldPlaces>,
    ),
    (Without<Dead>, Allow<Unpredicted>),
>;

/// The types of the riders the client holds, out of the world.
type RiderTypes<'w, 's> = Query<'w, 's, &'static UnitType, (Allow<Disabled>, Allow<Unpredicted>)>;

/// The nodes the panel writes each frame: the lines' texts and colors, the icons' tints, and the
/// bars' widths.
type PanelLines<'w, 's> = (
    Query<'w, 's, &'static mut Text>,
    Query<'w, 's, &'static mut TextColor>,
    Query<'w, 's, &'static mut ImageNode>,
    Query<'w, 's, &'static mut Node, Without<ButtonCell>>,
);

/// The panel's cells: its buttons', its queue's, its site's row, and its tooltip's line.
type PanelCells<'w, 's> = (
    Query<'w, 's, (Entity, &'static ButtonCell)>,
    Query<'w, 's, &'static QueueCell>,
    Query<'w, 's, &'static SiteRow>,
    Query<
        'w,
        's,
        (Entity, &'static mut Visibility),
        (With<Tooltip>, Without<Panel<CommandPanelPlugin>>),
    >,
);

/// What a press of a button orders and starts: the selection, which a command orders, the
/// buildings' angles, which a build's preview takes, the orders sent, and the input's mode.
#[derive(SystemParam)]
struct Presses<'w, 's> {
    selection: Res<'w, Selection>,
    looks: Res<'w, UnitLooks>,
    orders: ResMut<'w, PendingOrders>,
    mode: ResMut<'w, NextState<InputMode>>,
    commands: Commands<'w, 's>,
}

/// A cell's width and height in pixels, as Zero Hour's buttons of 50 by 44 at 800 by 600, and the
/// gap between cells.
const CELL: [f32; 2] = [56.0, 48.0];
const GAP: f32 = 3.0;
/// The panel's padding on each side, as the panels' style gives it.
const PADDING: f32 = 8.0;
/// A cell's background, and the bar of the head's progress.
const CELL_BACK: Color = Color::srgba(0.15, 0.15, 0.2, 0.9);
const BAR: Color = Color::srgb(0.3, 0.85, 0.4);
/// The tint of a dimmed icon.
const DIMMED_ICON: Color = Color::srgb(0.35, 0.35, 0.35);
/// The height of the bar of progress, in pixels.
const BAR_HEIGHT: f32 = 5.0;
/// The size of a name in a cell of no icon.
const CELL_FONT: f32 = 11.0;

impl Plugin for CommandPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shown>();
        app.init_resource::<Hovered>();
        app.init_resource::<Pressable>();
        app.add_systems(
            Startup,
            (Self::set_up, Self::load_looks, Self::spawn).chain(),
        );
        app.add_systems(PreUpdate, Self::press_keys.after(InputSystems));
        app.add_systems(OnExit(InputMode::Aim), Self::end_aiming);
        app.add_systems(
            Update,
            (
                Self::show.after(SelectionSystems),
                Self::aim
                    .run_if(in_state(InputMode::Aim))
                    .after(SelectionSystems)
                    .after(OrderSystems),
            ),
        );
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

    /// Loads the buttons' looks, as the start reads the client files.
    fn load_looks(
        (origins, names): (Res<'_, TypeOriginBook>, Res<'_, MatchNames>),
        (load, hud, text): (Res<'_, ClientLoad>, Res<'_, MatchHud>, Res<'_, EngineText>),
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        let sources = LookSources {
            origins: &origins,
            names: &names,
            load: &load,
            hud: &hud.0,
            text: &text,
        };
        let looks = PanelLooks::of(sources, |package, path| {
            assets.load(AssetPath::from(PackageAssets::asset_path(package, path)))
        });
        commands.insert_resource(looks);
    }

    /// Spawns the panel, as wide as its grid, and its tooltip over it.
    fn spawn(looks: Res<'_, PanelLooks>, mut commands: Commands<'_, '_>) {
        let columns = f32::from(looks.grid().columns.get());
        let width = columns * CELL[0] + (columns - 1.0) * GAP + 2.0 * PADDING;
        let root = Panel::<Self>::spawn(&mut commands, Corner::BottomRight, width);
        let over = Node {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(100.0),
            right: Val::Px(0.0),
            margin: UiRect::bottom(Val::Px(GAP)),
            padding: UiRect::all(Val::Px(PADDING)),
            ..Node::default()
        };
        commands.spawn((
            Text::default(),
            ui_style::font(),
            TextColor(TEXT),
            over,
            BackgroundColor(ui_style::BACK),
            Visibility::Hidden,
            Pickable::IGNORE,
            ChildOf(root),
            Tooltip,
        ));
    }

    /// Shows the unit the panel reads: its cells, made again when what it shows changes, and
    /// each frame the player's amounts, the buttons' dimming, the bar of the queue's head, the
    /// progress of a site, and the tooltip of the button under the cursor.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn show(
        own: OwnPlayer<'_, '_>,
        (selection, index, units, rider_types): (
            Res<'_, Selection>,
            Res<'_, EntityIndex>,
            PanelUnits<'_, '_>,
            RiderTypes<'_, '_>,
        ),
        (book, builds, researches, sales, names, trains): PanelBooks<'_>,
        (looks, hovered, mut pressable): (
            Res<'_, PanelLooks>,
            Res<'_, Hovered>,
            ResMut<'_, Pressable>,
        ),
        (tick, rate): (Res<'_, SimTick>, Res<'_, MatchRate>),
        (setup, requirements, cooldowns): (
            Res<'_, PanelSetup>,
            Option<Res<'_, RequirementBook>>,
            PlayerRows<'_, '_, PlayerCooldowns>,
        ),
        (mut shown, mut next): (ResMut<'_, Shown>, Local<'_, Shown>),
        (root, rows, funds): PanelNodes<'_, '_>,
        (mut texts, mut colors, mut icons, mut bars): PanelLines<'_, '_>,
        (cells, queue_cells, site_rows, mut tooltips): PanelCells<'_, '_>,
        mut commands: Commands<'_, '_>,
        (mut line, mut riders): (Local<'_, String>, Local<'_, Vec<(StableId, UnitType)>>),
    ) {
        let player_slot = own.player_slot();
        let books = MakingBooks {
            actions: &book,
            builds: &builds,
            researches: researches.as_deref(),
            sales: sales.as_deref(),
        };
        next.clear();
        for &unit in selection.get() {
            let Some(Ok((owner, &unit_type, slots, queue, site, _, _, tags, _, held_riders, _))) =
                index.get(unit).map(|at| units.get(at))
            else {
                continue;
            };
            if player_slot.is_none() || owner.map(|owner| owner.player_slot()) != player_slot {
                continue;
            }
            riders.clear();
            let ridden = held_riders.into_iter().flat_map(Riders::iter);
            riders.extend(ridden.filter_map(|rider| {
                let unit_type = rider_types.get(index.get(rider.id)?).ok()?;
                Some((rider.id, *unit_type))
            }));
            let held = PanelUnit {
                unit_type,
                slots,
                queue,
                site,
                tags: tags.map_or_else(TagSet::default, |tags| tags.tags),
                riders: &riders,
            };
            if next.fill(unit, held, books, &looks) {
                break;
            }
        }
        // The cells of a panel made again this frame are the old ones until the frame's commands
        // apply, so their places name no shown button.
        let remade = *shown != *next;
        if remade {
            mem::swap(&mut *shown, &mut *next);
            commands.entity(*rows).despawn_related::<Children>();
            shown.spawn_cells(&mut commands, *rows, &looks, &names);
        }
        let mut root = root.into_inner();
        pressable.0.clear();
        let Some((id, unit)) = shown.unit.and_then(|id| Some((id, index.get(id)?))) else {
            root.set_if_neq(Visibility::Hidden);
            return;
        };
        root.set_if_neq(Visibility::Inherited);
        let Ok((_, _, slots, queue, site, progress, carried, _, times, _, places)) =
            units.get(unit)
        else {
            return;
        };
        let now = tick.start();
        line.clear();
        for (at, &resource) in setup.resources.iter().enumerate() {
            let amount = own.amount(resource);
            let gap = if at == 0 { "" } else { "  " };
            write!(line, "{gap}{amount}").expect("a String takes any text");
        }
        ui_style::set_text(&mut texts, *funds, &line);
        let hz = NonZeroU64::from(rate.hz()).get();
        let value = |button: Button| match button.does {
            Does::Action { slot, making, .. } => {
                let held = slots.and_then(|slots| slots.slot(slot));
                let ranked = held.and_then(|held| Some((book.get(held.action?)?, held.rank?)));
                let done = making.done(
                    id,
                    |modifier| own.holds(modifier),
                    |modifier, source| {
                        carried.is_some_and(|carried| carried.get(modifier, Some(source)).is_some())
                    },
                );
                // A cast waits for its slot's ready tick and its player's, where its player keeps
                // it.
                let ready = |action: ActionId| {
                    let unit = times
                        .and_then(|times| times.slot(slot))
                        .map(|time| time.ready_at);
                    let player = cooldowns
                        .of_player_slot(player_slot?)
                        .map(|held| held.ready_at(action));
                    unit.into_iter().chain(player).max()
                };
                match ranked {
                    _ if done => ButtonValue::Done,
                    Some(_)
                        if making == Making::Cast
                            && let Some(ready) = held.and_then(|held| ready(held.action?))
                            && now < ready =>
                    {
                        let left = ready.since(now).map_or(0, Ticks::get);
                        ButtonValue::Cooling {
                            seconds: left.div_ceil(hz),
                        }
                    }
                    Some((action, rank)) => ButtonValue::Costs {
                        costs: action.resource_cost(rank),
                        seconds: action.values(rank).windup.get().div_ceil(hz),
                    },
                    None => ButtonValue::Nothing,
                }
            }
            Does::Command(_) | Does::Exit { .. } => ButtonValue::Nothing,
        };
        pressable.0.extend(shown.buttons.iter().map(|&button| {
            button.meets(requirements.as_deref(), |held| own.holds(held))
                && button.enabled(value(button), |costs| own.affords(costs))
                && button.has_place(trains.as_deref(), places, queue)
        }));
        let mut tip = None;
        for (entity, cell) in cells.iter().filter(|_| !remade) {
            let enabled = pressable.0[cell.at];
            if let Ok(mut icon) = icons.get_mut(cell.face) {
                let tint = if enabled { Color::WHITE } else { DIMMED_ICON };
                if icon.color != tint {
                    icon.color = tint;
                }
            }
            if let Ok(mut color) = colors.get_mut(cell.face) {
                color.set_if_neq(TextColor(if enabled { TEXT } else { DIMMED }));
            }
            if hovered.0 == Some(entity) {
                let button = shown.buttons[cell.at];
                tip = Some((button, value(button)));
            }
        }
        if let Ok((tooltip, mut shows)) = tooltips.single_mut() {
            line.clear();
            if let Some((button, value)) = tip {
                let name = button.name(&looks, &names);
                let tip = ButtonTip {
                    name,
                    key: button.key,
                    value,
                };
                write!(line, "{tip}").expect("a String takes any text");
            }
            ui_style::set_text(&mut texts, tooltip, &line);
            shows.set_if_neq(if line.is_empty() {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        }
        for cell in &queue_cells {
            let head = (cell.place == 0).then_some(queue).flatten();
            let percent = head.and_then(|queue| head_percent(queue, now));
            if let Some(mut bar) = cell.bar.and_then(|bar| bars.get_mut(bar).ok()) {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a share from 0 to 100 is exact in f32"
                )]
                let width = Val::Percent(percent.unwrap_or(0) as f32);
                if bar.width != width {
                    bar.width = width;
                }
            }
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

    /// A left click on a button presses it, when its player may.
    fn click(
        click: On<'_, '_, PointerClick>,
        cells: Query<'_, '_, &ButtonCell>,
        (shown, pressable): (Res<'_, Shown>, Res<'_, Pressable>),
        mut presses: Presses<'_, '_>,
    ) {
        if click.button != PointerButton::Primary {
            return;
        }
        let (Ok(cell), Some(unit)) = (cells.get(click.entity), shown.unit) else {
            return;
        };
        if pressable.0.get(cell.at) == Some(&true) {
            presses.press(unit, shown.buttons[cell.at]);
        }
    }

    /// A key with no modifier that went down presses the shown button of that key, the first in
    /// the order of their places, when its player may; one it may not takes the key and does
    /// nothing. A key a shown button takes is the panel's alone, as the frame's other readers see
    /// no press of it.
    fn press_keys(
        mut keys: ResMut<'_, ButtonInput<KeyCode>>,
        (shown, pressable): (Res<'_, Shown>, Res<'_, Pressable>),
        mut presses: Presses<'_, '_>,
    ) {
        let Some(unit) = shown.unit else {
            return;
        };
        let keyed = shown.keyed(|key| InputMap::take_pressed(Chord::plain(key), &mut keys));
        for (at, button) in keyed {
            if pressable.0.get(at) == Some(&true) {
                presses.press(unit, button);
            }
        }
    }

    /// The cursor over a button's cell hovers it, and off it hovers nothing.
    fn hover(over: On<'_, '_, PointerOver>, mut hovered: ResMut<'_, Hovered>) {
        hovered.0 = Some(over.entity);
    }

    fn unhover(out: On<'_, '_, PointerOut>, mut hovered: ResMut<'_, Hovered>) {
        if hovered.0 == Some(out.entity) {
            hovered.0 = None;
        }
    }

    /// A left click on the ground, off the UI, orders the selection to the command at its point
    /// and ends the aiming; a right click or the drop chord ends it.
    fn aim(
        (buttons, keys, input): (
            Res<'_, ButtonInput<MouseButton>>,
            Res<'_, ButtonInput<KeyCode>>,
            Res<'_, InputMap>,
        ),
        pointer: Pointer<'_, '_>,
        (aiming, selection): (Res<'_, AimedCommand>, Res<'_, Selection>),
        (mut orders, mut voices): (ResMut<'_, PendingOrders>, MessageWriter<'_, SelectionVoice>),
        mut mode: ResMut<'_, NextState<InputMode>>,
    ) {
        if buttons.just_pressed(MouseButton::Right) || input.pressed(Act::DropBuild, &keys) {
            mode.set_if_different(InputMode::Select);
            return;
        }
        if !buttons.just_pressed(MouseButton::Left) || pointer.is_over_ui() {
            return;
        }
        let Some(ground) = pointer.ground() else {
            return;
        };
        let (Some(to), Some(at)) = (float_num::ground_point(ground), float_num::position(ground))
        else {
            return;
        };
        if let Some(order) = aiming.order(to, at, &selection) {
            orders.push(order);
            // An attack move answers as a move does (`pickAndPlayUnitVoiceResponse`).
            if *aiming == AimedCommand::AttackMove {
                voices.write(SelectionVoice::Move);
            }
        }
        mode.set_if_different(InputMode::Select);
    }

    /// Ends the aiming as the input's mode leaves `InputMode::Aim`.
    fn end_aiming(mut commands: Commands<'_, '_>) {
        commands.remove_resource::<AimedCommand>();
    }

    /// A left click on a cell of the queue cancels its entry.
    fn cancel_train(
        click: On<'_, '_, PointerClick>,
        cells: Query<'_, '_, &QueueCell>,
        shown: Res<'_, Shown>,
        orders: ResMut<'_, PendingOrders>,
    ) {
        if let Ok(cell) = cells.get(click.entity) {
            let place = cell.place;
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

impl Presses<'_, '_> {
    /// Presses `button` of `unit`'s.
    fn press(&mut self, unit: StableId, button: Button) {
        let angle = |building| self.looks.placement_angle(building);
        match button.pressed(unit, &self.selection, angle) {
            Some(Pressed::Order(order)) => self.orders.push(order),
            Some(Pressed::Place(placing)) => {
                self.commands.insert_resource(placing);
                self.mode.set_if_different(InputMode::Place);
            }
            Some(Pressed::Aim(aiming)) => {
                self.commands.insert_resource(aiming);
                self.mode.set_if_different(InputMode::Aim);
            }
            None => {}
        }
    }
}

impl Button {
    /// Its name: its action's, as `names` names it, or its command's.
    fn name<'a>(self, looks: &'a PanelLooks, names: &'a NameTexts<ActionId>) -> &'a str {
        match self.does {
            Does::Action { action, .. } => names.of(action),
            Does::Command(command) => &looks.command(command).name,
            Does::Exit { .. } => &looks.command(PanelCommand::Exit).name,
        }
    }

    /// Whether its player holds the modifiers its action requires, as `requirements` gives them
    /// and `holds` says of a modifier; a command needs none.
    fn meets(
        self,
        requirements: Option<&RequirementBook>,
        holds: impl Fn(ModifierId) -> bool,
    ) -> bool {
        match self.does {
            Does::Action { action, .. } => requirements
                .and_then(|book| book.of(action))
                .is_none_or(|required| required.modifiers.iter().all(|&held| holds(held))),
            Does::Command(_) | Does::Exit { .. } => true,
        }
    }

    /// Whether its unit, of `places` and `queue`, has a place free for its train at a place, as
    /// `trains` says it is one; any other button needs none (04-capabilities/production.md, A train
    /// at a place reserves it as it queues).
    fn has_place(
        self,
        trains: Option<&ById<ActionId, TrainSpec>>,
        places: Option<&HeldPlaces>,
        queue: Option<&TrainQueue>,
    ) -> bool {
        let Does::Action {
            action,
            making: Making::Train,
            ..
        } = self.does
        else {
            return true;
        };
        match (trains, places, queue) {
            (Some(trains), Some(places), Some(queue)) => places.admits(action, queue, trains),
            _ => true,
        }
    }

    /// Whether its player may press it, its cost `value`, as `affords` says of a cost: a command
    /// always; an action its player can pay, not a research held or an action unlearned.
    fn enabled(self, value: ButtonValue<'_>, affords: impl Fn(&[ResourceAmount]) -> bool) -> bool {
        match (self.does, value) {
            (Does::Command(_) | Does::Exit { .. }, _) => true,
            (Does::Action { .. }, ButtonValue::Costs { costs, .. }) => affords(costs),
            (
                Does::Action { .. },
                ButtonValue::Cooling { .. } | ButtonValue::Done | ButtonValue::Nothing,
            ) => false,
        }
    }

    /// What a press of it, a button of `unit`'s, does with `selection`, a build's building at the
    /// angle `angle` gives: a train or a research orders the unit; a build places its building;
    /// a sale and a stop order the selection, none for an empty one; an attack move and a rally
    /// aim.
    fn pressed(
        self,
        unit: StableId,
        selection: &Selection,
        angle: impl Fn(UnitType) -> Num,
    ) -> Option<Pressed> {
        Some(match self.does {
            Does::Action {
                slot,
                making: Making::Train | Making::Research(_),
                ..
            } => {
                let target = ActionTarget::None;
                Pressed::Order(Order::one(unit, OrdersDirective::Slot { slot, target }))
            }
            Does::Action {
                slot,
                making: Making::Cast,
                ..
            } => Pressed::Aim(AimedCommand::Cast { unit, slot }),
            Does::Action {
                slot,
                action,
                making: Making::Build(building),
            } => Pressed::Place(Placing {
                builder: unit,
                slot,
                action,
                building,
                angle: angle(building),
            }),
            Does::Command(PanelCommand::Sell) => {
                Pressed::Order(selection.order(ProductionDirective::Sell)?)
            }
            Does::Command(PanelCommand::Stop) => {
                Pressed::Order(selection.order(OrdersDirective::Stop)?)
            }
            Does::Command(PanelCommand::AttackMove) => Pressed::Aim(AimedCommand::AttackMove),
            Does::Command(PanelCommand::Rally) => Pressed::Aim(AimedCommand::Rally),
            Does::Command(PanelCommand::Evacuate) => {
                Pressed::Order(selection.order(OrdersDirective::Evacuate)?)
            }
            Does::Command(PanelCommand::Exit) => {
                unreachable!("an exit place lays a rider's button, or none")
            }
            Does::Exit { rider, .. } => {
                Pressed::Order(Order::one(unit, OrdersDirective::Exit { rider }))
            }
        })
    }
}

/// A system param holds borrows of the world, whose fields print nothing of use.
impl fmt::Debug for Presses<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Presses")
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
            orders.push(Order::one(unit, directive));
        }
    }

    /// Shows nothing.
    fn clear(&mut self) {
        self.unit = None;
        self.buttons.clear();
        self.queue.clear();
        self.site = None;
    }

    /// Shows `unit`, as `held`, `books` and `looks` give it, unless it is no site and has no
    /// button: then it shows nothing. Whether it shows the unit. A site shows its own row alone,
    /// as its actions wait for it to complete. A look's places lay its buttons: each action its
    /// unit's slots hold of a train, a build or a research, and each command its unit takes, a
    /// sale of a type that sells, a rally of a unit that trains, each exit place the next of a
    /// container's riders, and an evacuation of a container that holds one; a look of no places
    /// lays its unit's trains, builds and researches in slot order, from the grid's first place.
    fn fill(
        &mut self,
        unit: StableId,
        held: PanelUnit<'_>,
        books: MakingBooks<'_>,
        looks: &PanelLooks,
    ) -> bool {
        self.clear();
        if let Some(site) = held.site {
            self.unit = Some(unit);
            self.site = Some(site.action());
            return true;
        }
        let tags = held.tags;
        let slots = held.slots.into_iter().flat_map(ActionSlots::indexed);
        let actions = slots.filter_map(|(slot, held)| {
            let action = held.action?;
            let making = match books.actions.get(action)?.kind {
                ActionKind::Train => Making::Train,
                ActionKind::Build => Making::Build(books.builds.of(action)?.unit_type),
                ActionKind::Research => Making::Research(*books.researches?.get(action)?),
                ActionKind::Cast => Making::cast(books.actions.get(action)?.when, tags)?,
                _ => return None,
            };
            Some(Does::Action {
                slot,
                action,
                making,
            })
        });
        let key = |does: Does| match does {
            Does::Action { action, .. } => looks.action(action)?.key,
            Does::Command(command) => looks.command(command).face.key,
            Does::Exit { .. } => looks.command(PanelCommand::Exit).face.key,
        };
        let places = looks.places(held.unit_type);
        if places.is_empty() {
            let laid = actions
                .enumerate()
                .map_while(|(at, does)| Some((looks.cell_in_order(at)?, does)));
            self.buttons.extend(laid.map(|(cell, does)| Button {
                cell,
                does,
                key: key(does),
            }));
        } else {
            let actions: Vec<Does> = actions.collect();
            let sells = books
                .sales
                .is_some_and(|sales| sales.get(held.unit_type).is_some());
            let mut riders = held.riders.iter();
            for placed in places {
                let does =
                    match placed.button {
                        PlacedButton::Action(id) => actions.iter().copied().find(
                            |does| matches!(does, Does::Action { action, .. } if *action == id),
                        ),
                        PlacedButton::Command(PanelCommand::Sell) => {
                            sells.then_some(Does::Command(PanelCommand::Sell))
                        }
                        PlacedButton::Command(PanelCommand::Rally) => {
                            held.queue.map(|_| Does::Command(PanelCommand::Rally))
                        }
                        PlacedButton::Command(PanelCommand::Exit) => {
                            let rider = riders.next();
                            rider.map(|&(rider, unit_type)| Does::Exit { rider, unit_type })
                        }
                        PlacedButton::Command(PanelCommand::Evacuate) => (!held.riders.is_empty())
                            .then_some(Does::Command(PanelCommand::Evacuate)),
                        PlacedButton::Command(command) => Some(Does::Command(command)),
                    };
                if let Some(does) = does {
                    self.buttons.push(Button {
                        cell: placed.cell,
                        does,
                        key: key(does),
                    });
                }
            }
        }
        if self.buttons.is_empty() {
            return false;
        }
        self.unit = Some(unit);
        let queue = held.queue.map_or(&[][..], TrainQueue::entries);
        self.queue.extend(queue.iter().map(|queued| queued.action));
        true
    }

    /// Its buttons whose keys `take` says went down, each with its place among them, in the order
    /// of their places, `take` asked of each key once at most, so of buttons of one key the first
    /// alone.
    fn keyed<'a>(
        &'a self,
        mut take: impl FnMut(Key) -> bool + 'a,
    ) -> impl Iterator<Item = (usize, Button)> + 'a {
        let buttons = &self.buttons;
        buttons.iter().enumerate().filter_map(move |(at, &button)| {
            let key = button.key?;
            let first = !buttons[..at].iter().any(|earlier| earlier.key == Some(key));
            (first && take(key)).then_some((at, button))
        })
    }

    /// Spawns its cells in `rows`, each face as `looks` and `names` give it: its buttons in the
    /// grid, then its queue's entries in a line, then its site's row.
    fn spawn_cells(
        &self,
        commands: &mut Commands<'_, '_>,
        rows: Entity,
        looks: &PanelLooks,
        names: &NameTexts<ActionId>,
    ) {
        let grid = looks.grid();
        let count = |tracks: NonZeroU8| GridTrackRepetition::Count(tracks.get().into());
        let grid_node = Node {
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::px(count(grid.columns), CELL[0]),
            grid_template_rows: RepeatedGridTrack::px(count(grid.rows), CELL[1]),
            column_gap: Val::Px(GAP),
            row_gap: Val::Px(GAP),
            ..Node::default()
        };
        let grid_node = commands.spawn((grid_node, ChildOf(rows))).id();
        for (at, &button) in self.buttons.iter().enumerate() {
            let face = match button.does {
                Does::Action { action, .. } => looks.action(action).cloned(),
                Does::Command(command) => Some(looks.command(command).face.clone()),
                Does::Exit { unit_type, .. } => {
                    let exit = &looks.command(PanelCommand::Exit).face;
                    Some(
                        looks
                            .unit(unit_type)
                            .cloned()
                            .unwrap_or_else(|| exit.clone()),
                    )
                }
            };
            let name = button.name(looks, names);
            let cell = Node {
                grid_column: GridPlacement::start(i16::from(button.cell.column) + 1),
                grid_row: GridPlacement::start(i16::from(button.cell.row) + 1),
                ..cell_node(CELL)
            };
            let cell = commands
                .spawn((cell, BackgroundColor(CELL_BACK), ChildOf(grid_node)))
                .id();
            let face = spawn_face(commands, cell, face.unwrap_or_default(), name);
            commands
                .entity(cell)
                .insert(ButtonCell { at, face })
                .observe(CommandPanelPlugin::click)
                .observe(CommandPanelPlugin::hover)
                .observe(CommandPanelPlugin::unhover);
        }
        if !self.queue.is_empty() {
            let line = Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(GAP),
                ..Node::default()
            };
            let line = commands.spawn((line, ChildOf(rows))).id();
            for (place, &action) in (0..=u8::MAX).zip(&self.queue) {
                let cell = commands
                    .spawn((cell_node(CELL), BackgroundColor(CELL_BACK), ChildOf(line)))
                    .id();
                let face = looks.action(action).cloned().unwrap_or_default();
                spawn_face(commands, cell, face, names.of(action));
                let bar = (place == 0).then(|| {
                    let bar = Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        height: Val::Px(BAR_HEIGHT),
                        width: Val::Percent(0.0),
                        ..Node::default()
                    };
                    commands
                        .spawn((bar, BackgroundColor(BAR), ChildOf(cell)))
                        .id()
                });
                commands
                    .entity(cell)
                    .insert(QueueCell { place, bar })
                    .observe(CommandPanelPlugin::cancel_train);
            }
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

/// A cell's node of `size` pixels, its face centered in it.
fn cell_node(size: [f32; 2]) -> Node {
    Node {
        width: Val::Px(size[0]),
        height: Val::Px(size[1]),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..Node::default()
    }
}

/// Spawns in `cell` the face `face`: its icon, filling the cell, or else `name`; the face's node.
fn spawn_face(commands: &mut Commands<'_, '_>, cell: Entity, face: CellFace, name: &str) -> Entity {
    match face.icon {
        Some(icon) => {
            let image = ImageNode {
                image: icon.image,
                rect: Some(icon.rect),
                ..ImageNode::default()
            };
            let fill = Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..Node::default()
            };
            commands.spawn((image, fill, ChildOf(cell))).id()
        }
        None => commands
            .spawn((
                Text::new(name),
                TextFont::from_font_size(CELL_FONT),
                TextColor(TEXT),
                ChildOf(cell),
            ))
            .id(),
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

    /// A cast's button, while its unit's `tags` pass its action's `when`; none, hidden, while
    /// they do not.
    fn cast(when: Option<Filter>, tags: TagSet) -> Option<Self> {
        when.is_none_or(|when| when.selects_tags(tags))
            .then_some(Self::Cast)
    }
}

impl fmt::Display for ButtonValue<'_> {
    /// Each cost's amount, then the seconds, `N s`; a cooling cast's `ready in N s`; `done`; or
    /// nothing.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Costs { costs, seconds } => {
                for cost in *costs {
                    write!(f, "{}  ", cost.amount)?;
                }
                write!(f, "{seconds} s")
            }
            Self::Cooling { seconds } => write!(f, "ready in {seconds} s"),
            Self::Done => f.write_str("done"),
            Self::Nothing => Ok(()),
        }
    }
}

impl fmt::Display for ButtonTip<'_> {
    /// Its name, its key's cap in brackets, then its cost, each after two spaces.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)?;
        if let Some(key) = self.key {
            write!(f, "  [{}]", Chord::plain(key).label())?;
        }
        match self.value {
            ButtonValue::Nothing => Ok(()),
            value => write!(f, "  {value}"),
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
