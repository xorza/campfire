use campfire_codec::{Binary, BinaryError};

use super::*;

/// A grid of 3 × 2 samples, two cells, with an overlay over the second; a texture of 8 × 8
/// texels at two levels, of one copy and its border.
fn parts() -> (TerrainQuads, TerrainTexture, TerrainSurface) {
    let quad = |[left, right]: [f32; 2], flip: bool| GroundQuad {
        uv: [
            Vec2::new(left, 0.0),
            Vec2::new(right, 0.0),
            Vec2::new(right, 1.0),
            Vec2::new(left, 1.0),
        ],
        flip,
    };
    let quads = TerrainQuads {
        columns: 3,
        ground: vec![quad([0.0, 1.0], false), quad([2.0, 3.0], true)],
        overlays: vec![Overlay {
            cell: 1,
            uv: [Vec2::new(4.0, 2.0); 4],
            alpha: [0, 255, 255, 0],
            flip: false,
        }],
    };
    let texture = TerrainTexture {
        size: UVec2::new(8, 8),
        levels: 2,
        copies: vec![TextureCopy {
            texture: PackagePath::parse("client/textures/sand.ktx2").unwrap(),
            from: UVec2::new(0, 2),
            to: UVec2::new(2, 2),
            size: UVec2::new(4, 2),
        }],
        wraps: vec![BorderWrap {
            at: UVec2::new(2, 2),
            size: UVec2::new(4, 2),
            border: 2,
        }],
    };
    let surface = TerrainSurface {
        metallic: 0.0,
        roughness: 1.0,
        specular: 0.0,
    };
    (quads, texture, surface)
}

#[test]
fn a_terrain_reads_back_what_it_writes_and_each_flaw_is_refused() {
    let (quads, texture, surface) = parts();
    let file = TerrainFile::new(quads, texture, surface).unwrap();
    let read: TerrainFile = Binary::decode(&Binary::encode(&file)).unwrap();
    assert_eq!(read, file);
    // Its two cells are those of a grid of 3 × 2 samples, not of 3 × 3 or of 2 × 2.
    assert!(file.quads().fits(3, 2));
    assert!(!file.quads().fits(3, 3));
    assert!(!file.quads().fits(2, 2));
    let refused = |change: fn(&mut TerrainQuads, &mut TerrainTexture, &mut TerrainSurface)| {
        let (mut quads, mut texture, mut surface) = parts();
        change(&mut quads, &mut texture, &mut surface);
        TerrainFile::new(quads, texture, surface).unwrap_err()
    };
    // A grid of one column has no cell; three quads fill no whole rows of two cells.
    assert_eq!(refused(|q, _, _| q.columns = 1), TerrainFileError::Shape);
    assert_eq!(
        refused(|q, _, _| q.ground.push(q.ground[0])),
        TerrainFileError::Shape
    );
    assert_eq!(refused(|q, _, _| q.ground.clear()), TerrainFileError::Shape);
    // An overlay of cell 2, past the two.
    assert_eq!(
        refused(|q, _, _| q.overlays[0].cell = 2),
        TerrainFileError::Cell(0)
    );
    assert_eq!(
        refused(|q, _, _| q.overlays[0].uv[3].y = f32::NAN),
        TerrainFileError::Coordinate
    );
    assert_eq!(
        refused(|q, _, _| q.ground[1].uv[0].x = f32::INFINITY),
        TerrainFileError::Coordinate
    );
    // No level; five levels of a side of 8, which halves to 1 at the fourth and 0 at the fifth.
    assert_eq!(refused(|_, t, _| t.levels = 0), TerrainFileError::Texture);
    assert_eq!(refused(|_, t, _| t.levels = 5), TerrainFileError::Texture);
    // A copy past the texture's 8 texels, of no texels, or of a place or a size off the even
    // texels the second level halves to; its source corner may be on any texel.
    assert_eq!(
        refused(|_, t, _| t.copies[0].to.x = 6),
        TerrainFileError::Copy(0)
    );
    assert_eq!(
        refused(|_, t, _| t.copies[0].size.y = 0),
        TerrainFileError::Copy(0)
    );
    assert_eq!(
        refused(|_, t, _| t.copies[0].to.y = 1),
        TerrainFileError::Copy(0)
    );
    assert_eq!(
        refused(|_, t, _| t.copies[0].size.x = 3),
        TerrainFileError::Copy(0)
    );
    let (quads, mut texture, surface) = parts();
    texture.copies[0].from = UVec2::new(1, 3);
    assert!(TerrainFile::new(quads, texture, surface).is_ok());
    // A border past the texture's edge, wider than its rectangle, or off the even texels.
    assert_eq!(
        refused(|_, t, _| t.wraps[0].at.x = 0),
        TerrainFileError::Wrap(0)
    );
    assert_eq!(
        refused(|_, t, _| t.wraps[0].border = 4),
        TerrainFileError::Wrap(0)
    );
    assert_eq!(
        refused(|_, t, _| t.wraps[0].border = 1),
        TerrainFileError::Wrap(0)
    );
    // A rectangle so wide that its border's end passes `u32`: 2³² − 2 + 2.
    assert_eq!(
        refused(|_, t, _| t.wraps[0].size.x = u32::MAX - 1),
        TerrainFileError::Wrap(0)
    );
    // A factor past 1, or not a number.
    assert_eq!(
        refused(|_, _, s| s.roughness = 1.5),
        TerrainFileError::Surface
    );
    assert_eq!(
        refused(|_, _, s| s.specular = f32::NAN),
        TerrainFileError::Surface
    );
    // A file read is checked as one made is.
    let bytes = Binary::encode(&file);
    let mut bad = file;
    bad.quads.overlays[0].cell = 9;
    assert_eq!(
        Binary::decode::<TerrainFile>(&Binary::encode(&bad)).err(),
        Some(BinaryError::REFUSED)
    );
    // A file is untrusted: each byte cut short is refused, and no flip of one panics.
    for at in 0..bytes.len() {
        let cut = Binary::decode::<TerrainFile>(&bytes[..at]);
        assert_eq!(
            cut.err(),
            Some(BinaryError::Truncated),
            "truncated to {at} bytes"
        );
        for flip in [0x01, 0x80, 0xFF] {
            let mut flawed = bytes.clone();
            flawed[at] ^= flip;
            Binary::decode::<TerrainFile>(&flawed).ok();
        }
    }
}
