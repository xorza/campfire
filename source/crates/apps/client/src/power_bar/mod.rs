use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{AssetPath, AssetServer};
use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::{With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, Single};
use bevy::math::Rect;
use bevy::ui::widget::{ImageNode, NodeImageMode};
use bevy::ui::{BackgroundColor, Node, PositionType, UiRect, Val};
use campfire_client_files::{PanelIcon, PowerBar};
use campfire_production::PlayerPower;
use campfire_values::PackageIndex;

use crate::own_player::OwnPlayer;
use crate::ui_style::BACK;
use crate::view::ViewSystems;
use crate::view::float_num::FloatNum;
use crate::view::match_hud::MatchHud;
use crate::view::package_source::PackageAssets;

/// The power bar, of a mode whose HUD file gives one: the player's power given as its fill, and its
/// power used as its needle, as `W3DPowerDraw` draws Zero Hour's (02-engine-core.md, The panel
/// shows the power bar); hidden until the player's power reaches the client.
#[derive(Debug)]
pub(crate) struct PowerBarPlugin;

/// The bar's size in pixels: Zero Hour's 278 by 6 of its control bar at 800 by 600, at the panel's
/// scale of its cells, 56 to the game's 50.
const SIZE: [f32; 2] = [278.0 * 56.0 / 50.0, 6.0 * 56.0 / 50.0];

/// The bar's root, its fill and its needle.
#[derive(Component, Debug)]
struct BarRoot;

#[derive(Component, Debug)]
struct BarFill;

#[derive(Component, Debug)]
struct BarNeedle;

/// Which fill the bar shows: of power enough with room, within the margin of the power given, or
/// short.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PowerState {
    Enough,
    Near,
    Short,
}

/// Where the bar draws, in pixels from its left: its fill's end, its needle's left edge, none for
/// no needle, and the fill it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BarDraw {
    fill: f32,
    needle: Option<f32>,
    state: PowerState,
}

/// The widths `BarDraw::of` draws by, in pixels: the bar's, a fill's tile's and the needle's.
#[derive(Debug, Clone, Copy)]
struct BarWidths {
    bar: f32,
    tile: f32,
    needle: f32,
}

impl Plugin for PowerBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::spawn);
        app.add_systems(Update, Self::show.after(ViewSystems));
    }
}

impl PowerBarPlugin {
    /// Spawns the bar, centred at the window's top, hidden, of a mode whose HUD file gives one.
    fn spawn(
        hud: Option<Res<'_, MatchHud>>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(bar) = hud.and_then(|hud| hud.0.power_bar.clone()) else {
            return;
        };
        let root = Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-SIZE[0] / 2.0)),
            width: Val::Px(SIZE[0]),
            height: Val::Px(SIZE[1]),
            ..Node::default()
        };
        let root = commands
            .spawn((root, BackgroundColor(BACK), Visibility::Hidden, BarRoot))
            .id();
        let image = |icon: &PanelIcon, mode| {
            let path = PackageAssets::asset_path(PackageIndex::MODE, &icon.texture);
            let [left, top, right, bottom] = icon.rect.edges().map(f32::from);
            ImageNode {
                image: assets.load(AssetPath::from(path)),
                rect: Some(Rect::new(left, top, right, bottom)),
                image_mode: mode,
                ..ImageNode::default()
            }
        };
        let tiled = NodeImageMode::Tiled {
            tile_x: true,
            tile_y: false,
            stretch_value: 1.0,
        };
        let placed = |width: f32| Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            width: Val::Px(width),
            height: Val::Percent(100.0),
            ..Node::default()
        };
        commands.spawn((
            image(&bar.fills().enough, tiled),
            placed(0.0),
            ChildOf(root),
            BarFill,
        ));
        let needle = Self::scaled(bar.needle().rect.edges());
        commands.spawn((
            image(bar.needle(), NodeImageMode::Stretch),
            placed(needle),
            Visibility::Hidden,
            ChildOf(root),
            BarNeedle,
        ));
        commands.insert_resource(BarLook(bar));
    }

    /// A rectangle's width of `edges` at the bar's scale.
    fn scaled(edges: [u16; 4]) -> f32 {
        f32::from(edges[2] - edges[0]) * SIZE[1] / f32::from(edges[3] - edges[1])
    }

    /// Shows the bar once the player's power reaches the client: its fill to its given power, of
    /// its state's tile, and its needle at its used power.
    fn show(
        look: Option<Res<'_, BarLook>>,
        own: OwnPlayer<'_, '_>,
        root: Option<RootShown<'_, '_>>,
        mut fill: FillNodes<'_, '_>,
        mut needle: NeedleNodes<'_, '_>,
        assets: Res<'_, AssetServer>,
    ) {
        let (Some(look), Some(mut root)) = (look, root) else {
            return;
        };
        let Some(power) = own.power() else {
            return;
        };
        root.set_if_neq(Visibility::Inherited);
        let bar = &look.0;
        let widths = BarWidths {
            bar: SIZE[0],
            tile: Self::scaled(bar.fills().enough.rect.edges()),
            needle: Self::scaled(bar.needle().rect.edges()),
        };
        let draw = BarDraw::of(bar, power, widths);
        if let Ok((mut node, mut image)) = fill.single_mut() {
            let width = Val::Px(draw.fill);
            if node.width != width {
                node.width = width;
            }
            let icon = match draw.state {
                PowerState::Enough => &bar.fills().enough,
                PowerState::Near => &bar.fills().near,
                PowerState::Short => &bar.fills().short,
            };
            let [left, top, right, bottom] = icon.rect.edges().map(f32::from);
            let rect = Some(Rect::new(left, top, right, bottom));
            if image.rect != rect {
                let path = PackageAssets::asset_path(PackageIndex::MODE, &icon.texture);
                image.image = assets.load(AssetPath::from(path));
                image.rect = rect;
            }
        }
        if let Ok((mut node, mut shown)) = needle.single_mut() {
            match draw.needle {
                Some(left) => {
                    let left = Val::Px(left);
                    if node.left != left {
                        node.left = left;
                    }
                    shown.set_if_neq(Visibility::Inherited);
                }
                None => {
                    shown.set_if_neq(Visibility::Hidden);
                }
            }
        }
    }
}

/// Whether the bar's root shows.
type RootShown<'w, 's> =
    Single<'w, 's, &'static mut Visibility, (With<BarRoot>, Without<BarNeedle>)>;

/// The fill's node and image.
type FillNodes<'w, 's> =
    Query<'w, 's, (&'static mut Node, &'static mut ImageNode), (With<BarFill>, Without<BarNeedle>)>;

/// The needle's node and whether it shows.
type NeedleNodes<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, &'static mut Visibility),
    (With<BarNeedle>, Without<BarRoot>),
>;

/// The match's power bar, as its HUD file gives it.
#[derive(Resource, Debug)]
struct BarLook(PowerBar);

impl BarDraw {
    /// Where a bar of `bar`'s look and of `widths` draws `power`, as `W3DPowerDraw` does: the
    /// given power's length is its logarithm of the bar's base over its intervals of the width,
    /// at most the width; the fill draws whole tiles to it and one more, cut at the bar's end;
    /// the needle stands centred at the used power's length, a use of 1 taken as 1.5, as
    /// `log₇(1)` shows nothing, at most at the bar's end and at least at its start, and is none
    /// when the fill reached the bar's end or drew nothing and the use's length is none; the
    /// state is short when the use passes the power given, near when it lies within the margin
    /// below it or at it.
    fn of(bar: &PowerBar, power: PlayerPower, widths: BarWidths) -> Self {
        let (given, used) = (power.given().float(), power.used().float());
        let per_interval = widths.bar / bar.intervals().float();
        let base = f32::from(bar.base()).ln();
        let length = |value: f32| value.ln() / base * per_interval;
        let range = length(given).min(widths.bar);
        let (fill, rest) = if range > 0.0 {
            let whole = (range / widths.tile).floor() * widths.tile;
            let rest = widths.bar - whole;
            (whole + rest.min(widths.tile).max(0.0), rest)
        } else {
            (0.0, range)
        };
        #[expect(clippy::float_cmp, reason = "a use of exactly 1, as the game tests it")]
        let needled = if used == 1.0 { 1.5 } else { used };
        let at = length(needled);
        let needle = if rest <= 0.0 && at <= 0.0 {
            None
        } else if at >= widths.bar {
            Some(widths.bar - widths.needle)
        } else {
            Some((at - widths.needle / 2.0).max(0.0))
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "a margin of a few units of power is exact in f32"
        )]
        let margin = bar.margin() as f32;
        let state = if used > given {
            PowerState::Short
        } else if used > given - margin {
            PowerState::Near
        } else {
            PowerState::Enough
        };
        Self {
            fill,
            needle,
            state,
        }
    }
}

#[cfg(test)]
mod tests;
