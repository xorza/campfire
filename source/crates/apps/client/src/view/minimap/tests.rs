use bevy::asset::AssetApp;
use bevy::math::UVec2;
use campfire_client_files::GroundQuad;
use campfire_math::Num;

use super::*;
use crate::test_app;

#[test]
fn a_frame_places_its_cells_on_the_ground_and_a_point_in_its_cell() {
    // 4 cells over x 10 to 50 by z −20 to 20: cells of 10 m.
    let frame = Frame {
        corner: Vec2::new(10.0, -20.0),
        size: Vec2::new(40.0, 40.0),
    };
    assert_eq!(frame.center(4, 0), Vec2::new(15.0, -15.0));
    assert_eq!(frame.center(4, 6), Vec2::new(35.0, -5.0));
    assert_eq!(frame.cell_of(4, Vec2::new(35.0, -5.0)), Some([2, 1]));
    assert_eq!(frame.cell_of(4, Vec2::new(9.0, 0.0)), None);
    assert_eq!(frame.cell_of(4, Vec2::new(50.0, 0.0)), None);
    // A click halfway across and a quarter down names (30, −10).
    assert_eq!(frame.point_at(Vec2::new(0.5, 0.25)), Vec2::new(30.0, -10.0));
    // A point past the grid keeps its cell outside, for a line to its edge.
    assert_eq!(frame.cells_at(4, Vec2::new(-10.0, 60.0)), [-2, 8]);
}

#[test]
fn a_square_and_a_line_fill_their_cells_within_the_grid() {
    let mut texels = vec![0; 4 * 4 * 4];
    let lit = |texels: &[u8]| -> Vec<usize> { (0..16).filter(|&at| texels[at * 4] == 9).collect() };
    // A square of 2 at the last column is cut at the grid's edge.
    square(&mut texels, 4, [3, 1], 2, [9, 9, 9]);
    assert_eq!(lit(&texels), [7, 11]);
    // A line from (−1, 0) to (3, 0) fills the row's cells within the grid; one down a diagonal
    // steps a cell each.
    let mut texels = vec![0; 4 * 4 * 4];
    line(&mut texels, 4, [-1, 0], [3, 0], [9, 9, 9]);
    assert_eq!(lit(&texels), [0, 1, 2, 3]);
    let mut texels = vec![0; 4 * 4 * 4];
    line(&mut texels, 4, [0, 0], [3, 3], [9, 9, 9]);
    assert_eq!(lit(&texels), [0, 5, 10, 15]);
}

#[test]
fn a_cell_takes_the_terrain_s_color_darkened_by_its_ground_below_the_highest() {
    // A grid of 3 × 3 samples 10 m apart, its last row 100 m high, the rest 0; the texture one
    // color, (200, 100, 50). A minimap of 2 cells over its 20 m: the centers of row 0, z 5, take
    // the sample of row 1, at 0, 100 m below the highest, so 60 % darker: (80, 40, 20); row 1,
    // z 15, takes the last row's, the highest, and keeps its color.
    let samples = [0, 0, 0, 0, 0, 0, 100, 100, 100].to_vec();
    let grid = HeightGrid::new([Num::ZERO; 2], Num::int(10), Num::ONE, 3, samples).unwrap();
    let image = TerrainImage::of_texels(UVec2::new(2, 2), [200, 100, 50, 255].repeat(4));
    let quad = GroundQuad {
        uv: [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y],
        flip: false,
    };
    let quads = TerrainQuads {
        columns: 3,
        ground: vec![quad; 4],
        overlays: Vec::new(),
    };
    let look = Minimap {
        cells: std::num::NonZeroU16::new(2).unwrap(),
        ..Minimap::default()
    };
    let terrain = TerrainSource {
        image: &image,
        quads: &quads,
        grid: &grid,
    };
    let ground = MinimapGround::of(terrain, &look);
    let dark = [80, 40, 20];
    let lit = [200, 100, 50];
    assert_eq!(ground.colors, [dark, dark, lit, lit]);
    assert_eq!(ground.frame.size, Vec2::new(20.0, 20.0));
}

#[test]
fn the_minimap_writes_its_image_only_as_what_it_draws_changes() {
    // A minimap of 2 × 2 cells of one color, no fog and no unit, its texels zeroed after each
    // frame: a frame that changes nothing writes none, and a camera that moves, or a unit that
    // dies or lives again, writes them all.
    let mut app = test_app::view();
    app.init_asset::<Image>();
    app.add_systems(Update, MinimapView::draw);
    let size = Extent3d {
        width: 2,
        height: 2,
        depth_or_array_layers: 1,
    };
    let blank = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0; 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD,
    );
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(blank);
    app.insert_resource(MinimapGround {
        side: 2,
        frame: Frame {
            corner: Vec2::ZERO,
            size: Vec2::splat(20.0),
        },
        colors: vec![[200, 100, 50]; 4],
    });
    app.insert_resource(MinimapView {
        look: Minimap::default(),
        image: image.clone(),
        radar: Radar::None,
        fog_cells: Vec::new(),
        teams: Vec::new(),
        shown: Vec::new(),
    });
    let camera = app
        .world_mut()
        .spawn((Camera3d::default(), GlobalTransform::default()))
        .id();
    app.world_mut().spawn((Visibility::Inherited, MinimapNode));
    let written = |app: &mut App| {
        app.update();
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let mut target = images.get_mut(&image).unwrap();
        let texels = target.data.as_mut().unwrap();
        let drawn = texels.clone();
        texels.fill(0);
        drawn
    };
    assert_eq!(written(&mut app), [200, 100, 50, 255].repeat(4));
    assert_eq!(written(&mut app), [0; 16]);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_xyz(1.0, 0.0, 0.0));
    assert_eq!(written(&mut app), [200, 100, 50, 255].repeat(4));
    let unit = app.world_mut().spawn(Dead).id();
    assert_eq!(written(&mut app), [200, 100, 50, 255].repeat(4));
    assert_eq!(written(&mut app), [0; 16]);
    app.world_mut().entity_mut(unit).remove::<Dead>();
    assert_eq!(written(&mut app), [200, 100, 50, 255].repeat(4));
}
