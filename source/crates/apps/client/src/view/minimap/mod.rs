use std::num::NonZeroUsize;

use bevy::app::{App, Plugin, PostStartup, Update};
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::camera::visibility::Visibility;
use bevy::camera::{Camera, Camera3d};
use bevy::color::ColorToPacked;
use bevy::ecs::change_detection::{DetectChanges, DetectChangesMut, Ref};
use bevy::ecs::component::Component;
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::{Added, Allow, Changed, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::image::{Image, ImageSampler};
use bevy::input::ButtonInput;
use bevy::input::mouse::MouseButton;
use bevy::math::{Vec2, Vec3};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::components::GlobalTransform;
use bevy::ui::widget::ImageNode;
use bevy::ui::{Node, PositionType, RelativeCursorPosition, Val};
use campfire_client_files::{Minimap, RadarShown, TerrainQuads};
use campfire_names::MatchNames;
use campfire_net::PendingOrders;
use campfire_orders::OrdersDirective;
use campfire_sim::{Position, Unpredicted};
use campfire_units::{Dead, HeightGrid, Owner, TypeOriginBook};
use campfire_values::{NameId, Tag, Team, UnitTags, UnitType};
use campfire_vision::{CellBits, ExploredCells, SeenCells};

use crate::own_player::OwnPlayer;
use crate::selection::{Selection, SelectionVoice};
use crate::view::camera_rig::CameraRig;
use crate::view::client_data::ClientLoad;
use crate::view::float_num::{FloatNum, ground_point};
use crate::view::fog::{FogSight, MatchFog};
use crate::view::match_colors::MatchColors;
use crate::view::match_hud::MatchHud;
use crate::view::terrain_image::TerrainImage;

/// The minimap's terrain, as the client composes it at load (02-engine-core.md, The minimap is
/// drawn from the terrain at load): its cells across and down, the frame that places them on the
/// ground, and each cell's color, row 0 at the least z, as `W3DRadar::buildTerrainTexture` gives
/// each radar cell the terrain's color averaged over the cells round it and darkened by its height.
#[derive(Resource, Debug, Clone, PartialEq)]
pub(crate) struct MinimapGround {
    side: usize,
    frame: Frame,
    colors: Vec<[u8; 3]>,
}

/// What the minimap's terrain is composed of: the terrain's composed texture, its ground quads, and
/// the height grid they lie on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainSource<'t> {
    pub(crate) image: &'t TerrainImage,
    pub(crate) quads: &'t TerrainQuads,
    pub(crate) grid: &'t HeightGrid,
}

/// Where a square grid of `side` cells stands on the ground: its corner of the least x and z, and
/// its width and depth, in meters.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame {
    corner: Vec2,
    size: Vec2,
}

/// The minimap as the client shows it: its look, its image, which it writes again as what it
/// draws changes, its radar, and each cell's fog cell, once the fog loaded.
#[derive(Resource, Debug)]
pub(crate) struct MinimapView {
    look: Minimap,
    image: Handle<Image>,
    radar: Radar,
    fog_cells: Vec<Option<usize>>,
    /// The sRGB colors of the match's teams, by their places.
    teams: Vec<[u8; 3]>,
    /// Who sees a unit of each type on the minimap, by the type's place.
    shown: Vec<RadarShown>,
}

/// What shows the minimap: nothing, as a mode of no radar tag; its tag, once the match's names
/// resolve it; or a tag the match lacks, which shows it never; or a tag not yet resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Radar {
    None,
    Unresolved,
    Tag(Tag),
    Unknown,
}

/// On the minimap's node.
#[derive(Component, Debug)]
struct MinimapNode;

/// The minimap: a node at the window's bottom left of the terrain, the fog, the units its player
/// sees and the camera's view, shown while its player holds a radar, which a click moves the camera
/// from and a right click orders the selection's move from (02-engine-core.md, A minimap click
/// moves the camera, a right click orders a move).
#[derive(Debug)]
pub(crate) struct MinimapPlugin;

/// The side of the minimap's node, in pixels, and its inset from the window's corner.
const SIDE_PX: f32 = 256.0;
const INSET_PX: f32 = 12.0;
/// The color of the camera's view on the minimap.
const VIEW: [u8; 3] = [255, 255, 255];

/// The units the minimap draws whose place, team or owner changed, or that died, since its last
/// frame; those that left or lived again are read apart, as their parts went.
type ChangedUnits<'w, 's> = Query<
    'w,
    's,
    (),
    (
        Or<(
            Changed<Position>,
            Changed<Team>,
            Changed<Owner>,
            Added<Dead>,
        )>,
        Allow<Unpredicted>,
    ),
>;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, MinimapView::spawn.before(ClientLoad::drop));
        app.add_systems(
            Update,
            (MinimapView::show, MinimapView::draw, MinimapView::click)
                .chain()
                .run_if(resource_exists::<MinimapView>),
        );
    }
}

impl MinimapGround {
    /// The minimap of `look`'s cells over `grid`, colored from `image` by the ground `quads` lay
    /// on it: each cell the texel its center's quad shows there, averaged with the cells round it,
    /// then darkened by `look`'s share as its ground lies below the map's highest, as the release
    /// game darkens a radar cell by its height below the map's highest (`interpolateColorForHeight`
    /// called with the map's highest as its middle).
    #[expect(
        clippy::cast_sign_loss,
        reason = "each place and each color is cast once it stands at 0 or more"
    )]
    pub(crate) fn of(terrain: TerrainSource<'_>, look: &Minimap) -> Self {
        let TerrainSource { image, quads, grid } = terrain;
        let side = NonZeroUsize::from(look.cells).get();
        let [ox, oz] = grid.origin().map(FloatNum::float);
        let [fx, fz] = grid.far_corner().map(FloatNum::float);
        let frame = Frame {
            corner: Vec2::new(ox.min(fx), oz.min(fz)),
            size: Vec2::new((fx - ox).abs(), (fz - oz).abs()),
        };
        let cell = grid.cell().float();
        let step = grid.step().float();
        let columns = grid.columns() as usize;
        let samples = grid.samples();
        let height = |at: Vec2| {
            let column = (((at.x - ox) / cell).round().max(0.0) as usize).min(columns - 1);
            let rows = samples.len() / columns;
            let row = (((at.y - oz) / cell).round().max(0.0) as usize).min(rows - 1);
            f32::from(samples[row * columns + column]) * step
        };
        let [lowest, highest] = samples.iter().fold([u8::MAX, 0], |[low, high], &sample| {
            [low.min(sample), high.max(sample)]
        });
        let [lowest, highest] = [lowest, highest].map(|sample| f32::from(sample) * step);
        let texels: Vec<[u8; 3]> = (0..side * side)
            .map(|at| image.texel(Self::uv(quads, grid, frame.center(side, at))))
            .collect();
        let dark = f32::from(look.dark) / 100.0;
        let colors = (0..side * side)
            .map(|at| {
                let (column, row) = (at % side, at / side);
                let mut sum = [0_u32; 3];
                let mut count = 0;
                for near_row in row.saturating_sub(1)..=(row + 1).min(side - 1) {
                    for near in column.saturating_sub(1)..=(column + 1).min(side - 1) {
                        let texel = texels[near_row * side + near];
                        for (sum, channel) in sum.iter_mut().zip(texel) {
                            *sum += u32::from(channel);
                        }
                        count += 1;
                    }
                }
                let below = if highest > lowest {
                    (highest - height(frame.center(side, at))) / (highest - lowest)
                } else {
                    0.0
                };
                let light = 1.0 - dark * below;
                sum.map(|sum| (sum as f32 / count as f32 * light).round() as u8)
            })
            .collect();
        Self {
            side,
            frame,
            colors,
        }
    }

    /// The point of the terrain's texture the ground quad under `at` shows there, in texels.
    #[expect(
        clippy::cast_sign_loss,
        reason = "a place is cast once it is clamped at 0 or more"
    )]
    fn uv(quads: &TerrainQuads, grid: &HeightGrid, at: Vec2) -> Vec2 {
        let [ox, oz] = grid.origin().map(FloatNum::float);
        let cell = grid.cell().float();
        let cells = quads.columns.saturating_sub(1).max(1) as usize;
        let rows = (quads.ground.len() / cells).max(1);
        let along = |offset: f32, count: usize| {
            let at = (offset / cell).clamp(0.0, count as f32 - f32::EPSILON);
            (at.floor() as usize, at.fract())
        };
        let (column, across) = along(at.x - ox, cells);
        let (row, down) = along(at.y - oz, rows);
        let Some(quad) = quads.ground.get(row * cells + column) else {
            return Vec2::ZERO;
        };
        let [a, b, c, d] = quad.uv;
        let top = a.lerp(b, across);
        let bottom = d.lerp(c, across);
        top.lerp(bottom, down)
    }
}

impl Frame {
    /// The center of cell `at` of a grid of `side` cells across, row by row.
    fn center(self, side: usize, at: usize) -> Vec2 {
        let cell = Vec2::new((at % side) as f32 + 0.5, (at / side) as f32 + 0.5);
        self.corner + cell * self.size / side as f32
    }

    /// The cell of a grid of `side` cells across under `point`, by its column and row; none past
    /// the grid.
    #[expect(
        clippy::cast_sign_loss,
        reason = "a place is cast once it stands within 0 and the side"
    )]
    fn cell_of(self, side: usize, point: Vec2) -> Option<[usize; 2]> {
        let at = (point - self.corner) / self.size * side as f32;
        let inside = |value: f32| {
            (0.0..side as f32)
                .contains(&value)
                .then_some(value as usize)
        };
        Some([inside(at.x)?, inside(at.y)?])
    }

    /// The point of the ground at `share` of the way across and down.
    fn point_at(self, share: Vec2) -> Vec2 {
        self.corner + share * self.size
    }

    /// The cell under `point`, clamped to the grid's, by its column and row as signed numbers, so
    /// a line to a point past the minimap is drawn up to its edge.
    fn cells_at(self, side: usize, point: Vec2) -> [i64; 2] {
        let at = (point - self.corner) / self.size * side as f32;
        [at.x.floor() as i64, at.y.floor() as i64]
    }
}

#[expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
impl MinimapView {
    /// Spawns the minimap's node, hidden, and its image, of the HUD's look, on a map whose
    /// terrain gave its ground.
    fn spawn(
        (ground, hud): (Option<Res<'_, MinimapGround>>, Option<Res<'_, MatchHud>>),
        (colors, load, origins): (
            Res<'_, MatchColors>,
            Res<'_, ClientLoad>,
            Res<'_, TypeOriginBook>,
        ),
        mut images: ResMut<'_, Assets<Image>>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(ground) = ground else {
            return;
        };
        let look = hud.map_or_else(Minimap::default, |hud| hud.0.minimap.clone());
        let side = u32::try_from(ground.side).expect("a minimap's side fits u32");
        let size = Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        };
        let mut image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[0, 0, 0, u8::MAX],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::nearest();
        let image = images.add(image);
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(INSET_PX),
                bottom: Val::Px(INSET_PX),
                width: Val::Px(SIDE_PX),
                height: Val::Px(SIDE_PX),
                ..Node::default()
            },
            ImageNode::new(image.clone()),
            RelativeCursorPosition::default(),
            Visibility::Hidden,
            MinimapNode,
        ));
        let teams = colors
            .teams()
            .map(|color| {
                let [r, g, b, _] = color.to_srgba().to_u8_array();
                [r, g, b]
            })
            .collect();
        let shown = origins
            .iter()
            .map(|(_, origin)| {
                let look = load.of(origin.package).units.units().get(&*origin.name);
                look.map_or(RadarShown::Everyone, |look| look.radar)
            })
            .collect();
        let radar = match look.radar {
            Some(_) => Radar::Unresolved,
            None => Radar::None,
        };
        commands.insert_resource(Self {
            look,
            image,
            radar,
            fog_cells: Vec::new(),
            teams,
            shown,
        });
    }

    /// Shows the minimap while its player holds a unit of the radar tag, at all times in a mode
    /// of none; its tag resolved once the match's names come, a tag the match lacks showing none.
    fn show(
        (own, names): (OwnPlayer<'_, '_>, Option<Res<'_, MatchNames>>),
        units: Query<'_, '_, (&Owner, &UnitTags), (Without<Dead>, Allow<Unpredicted>)>,
        mut view: ResMut<'_, Self>,
        node: Single<'_, '_, &mut Visibility, With<MinimapNode>>,
    ) {
        if let (Radar::Unresolved, Some(names), Some(radar)) = (view.radar, names, &view.look.radar)
        {
            let tag = names.tags().named(radar.as_str());
            if tag.is_none() {
                tracing::error!(
                    radar = radar.as_str(),
                    "the minimap's radar names no tag of the match"
                );
            }
            view.radar = tag.map_or(Radar::Unknown, Radar::Tag);
        }
        let shown = match view.radar {
            Radar::None => true,
            Radar::Unresolved | Radar::Unknown => false,
            Radar::Tag(tag) => {
                let own = own.player_slot();
                units.iter().any(|(owner, tags)| {
                    Some(owner.player_slot()) == own && tags.tags.contains(tag)
                })
            }
        };
        let visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        node.into_inner().set_if_neq(visibility);
    }

    /// Writes the minimap's image again: the terrain, darkened by the fog, each unit its player's
    /// team sees as a square of its team's color, and the camera's view on the ground. It writes
    /// only as one of those changes, or as it shows again: an image written changes its texture,
    /// which the renderer uploads whole.
    fn draw(
        (ground, fog): (Res<'_, MinimapGround>, Option<Res<'_, MatchFog>>),
        mut view: ResMut<'_, Self>,
        mut images: ResMut<'_, Assets<Image>>,
        own: OwnPlayer<'_, '_>,
        (units, changed, mut gone, mut revived): (
            Query<
                '_,
                '_,
                (&Position, &Team, &UnitType, Option<&Owner>),
                (Without<Dead>, Allow<Unpredicted>),
            >,
            ChangedUnits<'_, '_>,
            RemovedComponents<'_, '_, Position>,
            RemovedComponents<'_, '_, Dead>,
        ),
        (colors, camera): (
            Res<'_, MatchColors>,
            Single<'_, '_, (Ref<'_, Camera>, Ref<'_, GlobalTransform>), With<Camera3d>>,
        ),
        node: Single<'_, '_, Ref<'_, Visibility>, With<MinimapNode>>,
    ) {
        // Each frame reads the units that left or lived again, so a frame that shows again reads
        // none of before.
        let units_changed =
            gone.read().count() > 0 || revived.read().count() > 0 || !changed.is_empty();
        if **node == Visibility::Hidden {
            return;
        }
        let (seen, explored) = (own.seen_cells(), own.explored_cells());
        let (camera, place) = &*camera;
        let changed = node.is_changed()
            || units_changed
            || colors.is_changed()
            || fog.as_ref().is_some_and(DetectChanges::is_changed)
            || camera.is_changed()
            || place.is_changed()
            || seen.as_ref().is_some_and(DetectChanges::is_changed)
            || explored.as_ref().is_some_and(DetectChanges::is_changed);
        if !changed {
            return;
        }
        let side = ground.side;
        let Self {
            image,
            fog_cells,
            teams,
            look,
            shown,
            ..
        } = &mut *view;
        if fog_cells.is_empty()
            && let Some(fog) = fog.as_deref()
        {
            fog_cells.extend((0..side * side).map(|at| {
                let center = ground.frame.center(side, at);
                fog.cell_at(Vec3::new(center.x, 0.0, center.y))
            }));
        }
        let none = CellBits::default();
        let sight = match own.player_slot() {
            None => FogSight::All,
            Some(_) => FogSight::Cells {
                seen: seen.as_deref().map_or(&none, SeenCells::cells),
                explored: explored.as_deref().map_or(&none, ExploredCells::cells),
            },
        };
        let Some(mut target) = images.get_mut(&*image) else {
            return;
        };
        let texels = target.data.as_mut().expect("a minimap holds its texels");
        for (at, (texel, color)) in texels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&ground.colors)
            .enumerate()
        {
            let cell = fog_cells.get(at).copied().flatten();
            let light = match (fog.as_deref(), cell) {
                (Some(fog), Some(cell)) => fog.light(sight, cell),
                _ => [u8::MAX; 3],
            };
            let lit: [u8; 3] = std::array::from_fn(|channel| {
                let product = u32::from(color[channel]) * u32::from(light[channel]);
                u8::try_from((product + 127) / 255)
                    .expect("a product of two bytes over 255 fits a byte")
            });
            *texel = [lit[0], lit[1], lit[2], u8::MAX];
        }
        let seen = seen.as_deref().map(SeenCells::cells);
        let own = own.player_slot();
        let dot = NonZeroUsize::from(look.dot).get();
        for (position, &team, unit_type, owner) in &units {
            let mine = owner.is_some_and(|owner| Some(owner.player_slot()) == own);
            let drawn = match shown.get(unit_type.index()).copied() {
                Some(RadarShown::Everyone) | None => true,
                Some(RadarShown::Owner) => mine,
                Some(RadarShown::Never) => false,
            };
            if !drawn {
                continue;
            }
            let at = position.get();
            let point = Vec2::new(at.x.float(), at.z.float());
            let point3 = Vec3::new(point.x, 0.0, point.y);
            let sees = match (seen, fog.as_deref()) {
                (Some(seen), Some(fog)) => fog.cell_at(point3).is_some_and(|cell| seen.holds(cell)),
                _ => true,
            };
            if !sees {
                continue;
            }
            if let Some(cell) = ground.frame.cell_of(side, point) {
                let color = teams
                    .get(colors.place(team))
                    .copied()
                    .unwrap_or([u8::MAX; 3]);
                square(texels, side, cell, dot, color);
            }
        }
        let corners = view_corners(camera, place);
        if let Some(corners) = corners {
            let cells = corners.map(|corner| ground.frame.cells_at(side, corner));
            for at in 0..4 {
                line(texels, side, cells[at], cells[(at + 1) % 4], VIEW);
            }
        }
    }

    /// Moves the camera to the point a click on the minimap names, and to each a drag begun on it
    /// passes, and orders the selection's move to the one a right click names.
    fn click(
        (buttons, node): (
            Res<'_, ButtonInput<MouseButton>>,
            Single<'_, '_, (&RelativeCursorPosition, &Visibility), With<MinimapNode>>,
        ),
        ground: Res<'_, MinimapGround>,
        (selection, mut orders, mut voices): (
            Res<'_, Selection>,
            ResMut<'_, PendingOrders>,
            MessageWriter<'_, SelectionVoice>,
        ),
        mut rig: ResMut<'_, CameraRig>,
        mut dragging: Local<'_, bool>,
    ) {
        let (cursor, visibility) = *node;
        if !buttons.pressed(MouseButton::Left) {
            *dragging = false;
        }
        if *visibility == Visibility::Hidden || !cursor.cursor_over() {
            return;
        }
        let Some(share) = cursor.normalized else {
            return;
        };
        // The node's normalized place runs from −0.5 to 0.5 across and down from its center.
        let point = ground.frame.point_at(share + Vec2::splat(0.5));
        // A drag moves the camera while it began on the minimap, not one begun on the ground.
        *dragging |= buttons.just_pressed(MouseButton::Left);
        if *dragging {
            rig.look_at(point);
        }
        if buttons.just_pressed(MouseButton::Right) {
            let to = ground_point(Vec3::new(point.x, 0.0, point.y));
            let order = to.and_then(|to| selection.order(OrdersDirective::Move { to }));
            if let Some(order) = order {
                orders.push(order);
                voices.write(SelectionVoice::Move);
            }
        }
    }
}

/// The points of the ground the corners of `camera`'s view meet at height 0, in turn round it;
/// none for a corner that looks at no ground.
fn view_corners(camera: &Camera, place: &GlobalTransform) -> Option<[Vec2; 4]> {
    let size = camera.logical_viewport_size()?;
    let corners = [
        Vec2::ZERO,
        Vec2::new(size.x, 0.0),
        size,
        Vec2::new(0.0, size.y),
    ];
    let mut points = [Vec2::ZERO; 4];
    for (point, corner) in points.iter_mut().zip(corners) {
        let ray = camera.viewport_to_world(place, corner).ok()?;
        let direction = Vec3::from(ray.direction);
        if direction.y >= 0.0 {
            return None;
        }
        let at = ray.origin + direction * (-ray.origin.y / direction.y);
        *point = Vec2::new(at.x, at.z);
    }
    Some(points)
}

/// Fills a square of `dot` cells at `cell` of `texels`, `side` cells across, with `color`.
fn square(texels: &mut [u8], side: usize, [column, row]: [usize; 2], dot: usize, color: [u8; 3]) {
    for near_row in row..(row + dot).min(side) {
        for near in column..(column + dot).min(side) {
            let at = (near_row * side + near) * 4;
            texels[at..at + 3].copy_from_slice(&color);
        }
    }
}

/// Draws the line from cell `from` to cell `to` of `texels`, `side` cells across, in `color`, each
/// cell of it within the grid, as Bresenham steps it.
fn line(texels: &mut [u8], side: usize, from: [i64; 2], to: [i64; 2], color: [u8; 3]) {
    let [mut x, mut y] = from;
    let (dx, dy) = ((to[0] - x).abs(), -(to[1] - y).abs());
    let (sx, sy) = ((to[0] - x).signum(), (to[1] - y).signum());
    let mut error = dx + dy;
    let side_i = i64::try_from(side).expect("a minimap's side fits i64");
    loop {
        if (0..side_i).contains(&x) && (0..side_i).contains(&y) {
            let at = (usize::try_from(y * side_i + x).expect("a cell within the grid")) * 4;
            texels[at..at + 3].copy_from_slice(&color);
        }
        if [x, y] == to {
            break;
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            x += sx;
        }
        if twice <= dx {
            error += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
mod tests;
