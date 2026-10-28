use std::fmt::Write as _;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use campfire_capabilities::{Directive, Order};
use campfire_client_files::PerksFile;
use campfire_common::DeclaredName;
use campfire_host::JoinState;
use campfire_mode::PlayerSlotPicks;
use campfire_net::PendingOrders;
use campfire_players::{OrderUnits, PlayerRows};
use campfire_progression::{
    Learner, PerkBook, PlayerExperience, PlayerPerks, PlayerPoints, ProgressionDirective,
};
use campfire_values::PerkId;

use crate::client_packages::ClientPackages;
use crate::input_map::{Act, InputMap};
use crate::own_player::OwnPlayer;
use crate::ui_style::{self, Corner, DIMMED, Panel, PanelFunds, PanelRow, PanelRows, TEXT};
use crate::view::ViewSystems;
use crate::view::client_data::MatchPerks;

/// The panel of perks, as Zero Hour's general's promotions: the perks chord, G of the engine's,
/// opens and closes it; it shows the own player's rank, its progress and its points above, then
/// each page of perks of its pick of the perks file's choice, its perks in their places' order,
/// each with its cost; a perk it may learn now is bright, one it holds says so, and any other is
/// dimmed; a click learns it (generals.md, The perk panel by a choice).
#[derive(Debug)]
pub(crate) struct PerkPanelPlugin;

/// A row of the panel: the perk it learns, and its lines of name and state.
#[derive(Component, Debug)]
struct PerkRow {
    perk: PerkId,
    name: Entity,
    state: Entity,
}

/// Whether the panel laid its rows, which it does once the player's pick is known.
#[derive(Resource, Debug, Default)]
struct Laid(bool);

/// What a perk is to its player now: one it may learn for its cost, one it holds, or one it may
/// not learn yet or ever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PerkState {
    Learnable(u32),
    Held,
    Locked,
}

/// What the panel reads of each player: its experience, its points and its perks.
type PlayerProgress<'w, 's> = (
    PlayerRows<'w, 's, PlayerExperience>,
    PlayerRows<'w, 's, PlayerPoints>,
    PlayerRows<'w, 's, PlayerPerks>,
);

/// The holder of the panel's rows, and its line of the rank and the points.
type PerkNodes<'w, 's> = (
    Single<'w, 's, Entity, With<PanelRows<PerkPanelPlugin>>>,
    Single<'w, 's, Entity, With<PanelFunds<PerkPanelPlugin>>>,
);

/// The lines the panel writes, their colors, and its rows.
type RowLines<'w, 's> = (
    Query<'w, 's, &'static mut Text>,
    Query<'w, 's, &'static mut TextColor>,
    Query<'w, 's, &'static PerkRow>,
);

impl Plugin for PerkPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Laid>();
        app.add_systems(Startup, Self::spawn);
        app.add_systems(
            Update,
            (Self::toggle.run_if(InputMap::on(Act::Perks)), Self::show)
                .chain()
                .after(ViewSystems),
        );
    }
}

impl PerkPanelPlugin {
    fn spawn(mut commands: Commands<'_, '_>) {
        Panel::<Self>::spawn(&mut commands, Corner::TopRight, 280.0);
    }

    fn toggle(mut panel: Single<'_, '_, &mut Visibility, With<Panel<Self>>>) {
        let shown = **panel == Visibility::Hidden;
        **panel = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    /// Shows the own player's perks while the panel is open: its rows, laid once its pick is
    /// known, and each row's state, with the rank, its progress and the points above.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn show(
        panel: Single<'_, '_, &Visibility, With<Panel<Self>>>,
        (own, join, held): (
            OwnPlayer<'_, '_>,
            Option<Res<'_, JoinState>>,
            Res<'_, ClientPackages>,
        ),
        (perks, book, mut laid): (
            Res<'_, MatchPerks>,
            Option<Res<'_, PerkBook>>,
            ResMut<'_, Laid>,
        ),
        (experience, points, holdings): PlayerProgress<'_, '_>,
        (rows, funds): PerkNodes<'_, '_>,
        (mut texts, mut colors, perk_rows): RowLines<'_, '_>,
        mut commands: Commands<'_, '_>,
        mut line: Local<'_, String>,
    ) {
        let (Some(file), Some(book), Some(player_slot)) =
            (perks.0.as_ref(), book, own.player_slot())
        else {
            return;
        };
        if !laid.0 {
            let picks = join.as_deref().map(JoinState::player_slot_picks);
            let choices = &held.packages.data().choices;
            let value = picks
                .and_then(|picks| PlayerSlotPicks::of_places(choices, picks).ok())
                .and_then(|picks| picks.value(choices, player_slot, &file.choice).cloned());
            let Some(value) = value else {
                return;
            };
            for perk in Self::laid(file, &value, &book) {
                let name = book.names().name(perk).unwrap_or_default();
                let PanelRow { row, name, value } = PanelRow::spawn(&mut commands, *rows, name);
                commands
                    .entity(row)
                    .insert(PerkRow {
                        perk,
                        name,
                        state: value,
                    })
                    .observe(Self::learn);
            }
            laid.0 = true;
            return;
        }
        if **panel == Visibility::Hidden {
            return;
        }
        let experience = experience
            .of_player_slot(player_slot)
            .map(PlayerExperience::get);
        let rank = experience.and_then(|experience| experience.iter().next());
        let points = points
            .of_player_slot(player_slot)
            .map(|points| points.get());
        line.clear();
        if let Some(rank) = rank {
            write!(line, "rank {} · {} xp", rank.level.get(), rank.xp)
                .expect("a String takes any text");
        }
        write!(line, " · {} points", points.unwrap_or(0)).expect("a String takes any text");
        ui_style::set_text(&mut texts, *funds, &line);
        let held = holdings
            .of_player_slot(player_slot)
            .map_or(&[][..], PlayerPerks::get);
        let learner = Learner {
            perks: held,
            points,
            experience,
        };
        for row in &perk_rows {
            let state = Self::state(&book, row.perk, learner);
            line.clear();
            match state {
                PerkState::Learnable(cost) => write!(line, "{cost}"),
                PerkState::Held => write!(line, "held"),
                PerkState::Locked => Ok(()),
            }
            .expect("a String takes any text");
            ui_style::set_text(&mut texts, row.state, &line);
            let color = if state == PerkState::Locked {
                DIMMED
            } else {
                TEXT
            };
            for line in [row.name, row.state] {
                if let Ok(mut held) = colors.get_mut(line) {
                    held.set_if_neq(TextColor(color));
                }
            }
        }
    }

    /// The perks of the pages of `value` in `file`, page after page, each page's in its places'
    /// order, as `book` names them; a perk the book lacks left out.
    fn laid(file: &PerksFile, value: &DeclaredName, book: &PerkBook) -> Vec<PerkId> {
        let mut laid = Vec::new();
        for page in file.pages(value) {
            let mut places: Vec<_> = page.places.iter().collect();
            places.sort_by_key(|place| place.place);
            laid.extend(
                places
                    .into_iter()
                    .filter_map(|place| book.names().named(place.perk.as_str())),
            );
        }
        laid
    }

    /// What `perk` of `book` is to a player that holds `held`.
    fn state(book: &PerkBook, perk: PerkId, held: Learner<'_>) -> PerkState {
        if held.perks.binary_search(&perk).is_ok() {
            return PerkState::Held;
        }
        book.learn_cost(perk, held)
            .map_or(PerkState::Locked, PerkState::Learnable)
    }

    /// The order that learns `perk`: one of no units, which its player carries out.
    fn order(perk: PerkId) -> Order {
        Order {
            units: OrderUnits::NONE,
            directive: Directive::from(ProgressionDirective::LearnPerk { perk }),
        }
    }

    /// A left click on a row learns its perk.
    fn learn(
        click: On<'_, '_, PointerClick>,
        rows: Query<'_, '_, &PerkRow>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if click.button != PointerButton::Primary {
            return;
        }
        if let Ok(row) = rows.get(click.entity) {
            orders.push(Self::order(row.perk));
        }
    }
}

#[cfg(test)]
mod tests;
