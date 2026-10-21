use super::*;

/// A terrain of 2 × 2 cells, each naming each list's one entry.
fn fields() -> TerrainFields {
    let cell = TerrainCell {
        tile: 7,
        blend: Some(0),
        extra_blend: None,
        cliff_uv: Some(0),
        cliff: true,
    };
    TerrainFields {
        cells: vec![cell; 4],
        tiles: 2,
        classes: vec![TextureClass {
            first_tile: 0,
            tiles: 2,
            width: 1,
            name: "Sand".to_owned(),
        }],
        edge_tiles: 0,
        edge_classes: Vec::new(),
        blends: vec![BlendTile {
            tile: 4,
            shape: BlendShape::LeftDiagonal { long: true },
            inverted: true,
            flipped: false,
            custom_edge: None,
        }],
        cliffs: vec![CliffUv {
            tile: 0,
            corners: [[0.0, 1.0], [0.0, 0.0], [1.5, 0.0], [1.5, 1.0]],
            flip: false,
            mutant: true,
        }],
    }
}

#[test]
fn a_terrain_holds_only_with_each_index_in_its_list() {
    let fields = fields();
    assert_eq!(Terrain::new(fields.clone(), 2).unwrap().fields(), &fields);
    // Tile 7 is source tile 1 of 2; tile 8 would be source tile 2. Each list holds one entry,
    // so index 1 is past it.
    let with = |change: fn(&mut TerrainFields)| {
        let mut fields = fields.clone();
        change(&mut fields);
        Terrain::new(fields, 2).unwrap_err()
    };
    assert_eq!(
        with(|fields| fields.cells.pop().map(drop).unwrap()),
        TerrainError::Shape
    );
    assert_eq!(
        Terrain::new(fields.clone(), 0).unwrap_err(),
        TerrainError::Shape
    );
    assert_eq!(
        with(|fields| fields.cells[3].tile = 8),
        TerrainError::Tile(8)
    );
    assert_eq!(
        with(|fields| fields.cells[1].blend = Some(1)),
        TerrainError::Blend(1)
    );
    assert_eq!(
        with(|fields| fields.cells[2].extra_blend = Some(1)),
        TerrainError::Blend(1)
    );
    assert_eq!(
        with(|fields| fields.cells[0].cliff_uv = Some(1)),
        TerrainError::CliffUv(1)
    );
    assert_eq!(
        with(|fields| fields.blends[0].tile = 9),
        TerrainError::Tile(9)
    );
    assert_eq!(
        with(|fields| fields.blends[0].custom_edge = Some(0)),
        TerrainError::EdgeClass(0)
    );
    assert_eq!(
        with(|fields| fields.cliffs[0].tile = 12),
        TerrainError::Tile(12)
    );
    // A class of the atlas's 28 tiles a side holds; of 29, past its grid, not.
    let wide = |width| {
        let mut fields = fields.clone();
        fields.classes[0].width = width;
        Terrain::new(fields, 2)
    };
    assert!(wide(28).is_ok());
    assert_eq!(wide(29).unwrap_err(), TerrainError::ClassWidth(0));
    assert_eq!(wide(u32::MAX).unwrap_err(), TerrainError::ClassWidth(0));
    // The class's tiles 0 and 1 are the terrain's 2; from 1 they would reach a third.
    assert_eq!(
        with(|fields| fields.classes[0].first_tile = 1),
        TerrainError::Class(0)
    );
    assert_eq!(
        with(|fields| fields.edge_classes.push(fields.classes[0].clone())),
        TerrainError::EdgeClassTiles(0)
    );
}
