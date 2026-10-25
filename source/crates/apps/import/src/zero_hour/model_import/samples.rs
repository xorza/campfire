use crate::zero_hour::format::w3d::hierarchy::{Hierarchy, Pivot};
use crate::zero_hour::format::w3d::hlod::{Hlod, SubObject};
use crate::zero_hour::format::w3d::w3d_file::samples::{box_chunk, emitter_chunk};
use crate::zero_hour::format::w3d::w3d_mesh::samples::SKIN_GEOMETRY;
use crate::zero_hour::format::w3d::w3d_mesh::{
    Indexing, MaterialPass, Shader, Stage, TWO_SIDED, Texture, VertexMaterial, W3dMesh,
};

/// `Tank.w3d`: the HLOD `TANK` on its hierarchy of a root, a turret a half turn about `z` at
/// `(1, 2, 3)`, and a flash bone at `(0, 1, 0)` on the turret. Its sub-objects: a hull of
/// vertex colors, coordinates of `NaN` and infinity at one vertex, and two triangles, one of a
/// clamped texture `Rock.tga` and one of none; on the turret a triangle with zero normals and a
/// vertex no triangle uses; on the flash bone an additive two-sided triangle of a texture no
/// archive holds, `Gone.tga`, and no coordinates; a box; a name of no render object; and a skin
/// on the turret, its second vertex on the flash bone.
pub(crate) fn tank() -> Vec<u8> {
    let pivot = |name: &str, parent, translation, rotation| Pivot {
        name: name.to_owned(),
        parent,
        translation,
        rotation,
    };
    let identity = [0.0, 0.0, 0.0, 1.0];
    let hierarchy = Hierarchy {
        name: "TANK".to_owned(),
        pivots: vec![
            pivot("ROOTTRANSFORM", None, [0.0; 3], identity),
            pivot("TURRET", Some(0), [1.0, 2.0, 3.0], [0.0, 0.0, 1.0, 0.0]),
            pivot("MUZZLEFX01", Some(1), [0.0, 1.0, 0.0], identity),
        ],
    };
    let [hull, turret, flash, skin] = tank_meshes();
    let sub = |pivot, name: &str| SubObject {
        pivot,
        name: name.to_owned(),
    };
    let hlod = Hlod {
        name: "TANK".to_owned(),
        hierarchy: "TANK".to_owned(),
        sub_objects: vec![
            sub(0, "TANK.HULL"),
            sub(1, "TANK.TURRET"),
            sub(2, "TANK.FLASH"),
            sub(0, "TANK.PICKBOX"),
            sub(0, "TANK.GHOST"),
            sub(0, "TANK.SKIN"),
        ],
        aggregates: vec![sub(2, "TANK.SMOKE")],
    };
    [
        hull.chunk(),
        turret.chunk(),
        flash.chunk(),
        skin.chunk(),
        box_chunk("TANK.PICKBOX"),
        hierarchy.chunk(),
        hlod.chunk_of(1),
    ]
    .concat()
}

/// The tank's meshes: its hull, its turret, its flash and its skin.
fn tank_meshes() -> [W3dMesh; 4] {
    let hull = W3dMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        triangles: vec![[0, 1, 2], [2, 1, 3]],
        shaders: vec![Shader::DEFAULT],
        materials: vec![VertexMaterial {
            diffuse: [255, 128, 0],
            ..VertexMaterial::DEFAULT
        }],
        textures: vec![Texture {
            name: "Rock.tga".to_owned(),
            flags: 0x8,
        }],
        passes: vec![MaterialPass {
            materials: Indexing::One(0),
            shaders: Indexing::One(0),
            colors: Some(vec![[10, 20, 30, 40]; 4]),
            stage: Some(Stage {
                textures: Indexing::Each(vec![0, Stage::NONE]),
                coordinates: vec![
                    [0.0, 0.0],
                    [1.0, 0.0],
                    [0.0, 1.0],
                    [f32::NAN, f32::INFINITY],
                ],
            }),
            detail: None,
        }],
        ..W3dMesh::named("TANK.HULL")
    };
    let turret = W3dMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [5.0, 5.0, 5.0],
        ],
        normals: vec![[0.0; 3], [0.0, -1.0, 0.0], [0.0, -1.0, 0.0], [0.0; 3]],
        triangles: vec![[0, 1, 2]],
        ..W3dMesh::named("TANK.TURRET")
    };
    let flash = W3dMesh {
        flags: TWO_SIDED,
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        triangles: vec![[0, 1, 2]],
        shaders: vec![Shader {
            depth_write: false,
            dest_blend: 1,
            ..Shader::DEFAULT
        }],
        materials: vec![VertexMaterial {
            emissive: [255, 0, 0],
            ..VertexMaterial::DEFAULT
        }],
        textures: vec![Texture {
            name: "Gone.tga".to_owned(),
            flags: 0,
        }],
        passes: vec![MaterialPass {
            materials: Indexing::One(0),
            shaders: Indexing::One(0),
            colors: None,
            stage: Some(Stage {
                textures: Indexing::One(0),
                coordinates: Vec::new(),
            }),
            detail: None,
        }],
        ..W3dMesh::named("TANK.FLASH")
    };
    let skin = W3dMesh {
        flags: SKIN_GEOMETRY,
        positions: vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        bones: Some(vec![1, 2, 1]),
        triangles: vec![[0, 1, 2]],
        ..W3dMesh::named("TANK.SKIN")
    };
    [hull, turret, flash, skin]
}

/// `Rock01.w3d`: one mesh of an empty container, a triangle 1 m tall.
pub(crate) fn rock() -> Vec<u8> {
    W3dMesh {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        normals: vec![[0.0, -1.0, 0.0]; 3],
        triangles: vec![[0, 1, 2]],
        ..W3dMesh::named("ROCK01")
    }
    .chunk()
}

/// `Prop.w3d`, a box alone, and `Spray.w3d`, an emitter.
pub(crate) fn nothing_drawn() -> [Vec<u8>; 2] {
    [box_chunk("PROP"), emitter_chunk("SPRAY")]
}
