use std::collections::BTreeSet;

use crate::gltf::glb_builder::GlbBuilder;
use crate::zero_hour::error::{ModelError, ZeroHourError};
use crate::zero_hour::format::w3d::hierarchy::Hierarchy;
use crate::zero_hour::format::w3d::hlod::{Hlod, SubObject};
use crate::zero_hour::format::w3d::w3d_file::W3dFile;
use crate::zero_hour::format::w3d::w3d_mesh::W3dMesh;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_import::model_builder::ModelBuilder;
use crate::zero_hour::model_import::model_textures::ModelTextures;
use crate::zero_hour::model_nodes::ModelNodes;

mod model_builder;
mod model_textures;
mod w3d_material;

/// A model the objects name, converted: its glTF, the nodes its default look reads, the textures
/// its meshes name that no archive holds, which it draws as the game's `MissingTexture`, and its
/// HLOD's aggregates, which it does not draw.
#[derive(Debug)]
pub(crate) struct ModelImport {
    pub(crate) glb: GlbBuilder,
    pub(crate) nodes: ModelNodes,
    pub(crate) missing_textures: BTreeSet<String>,
    pub(crate) aggregates: Vec<String>,
}

/// What a model's name gives, as `WW3DAssetManager::Create_Render_Obj` finds it.
#[derive(Debug)]
pub(crate) enum Converted {
    Model(Box<ModelImport>),
    /// `NULL`, or a collision box alone: a render object that draws nothing.
    Nothing,
    /// A particle emitter, which the importer does not convert.
    Emitter,
    /// No file of the install holds it.
    Missing,
}

/// The render object `Find_Prototype` always has, which draws nothing.
const NULL: &str = "null";

/// The render object a model's name finds in its file: an HLOD on its hierarchy, with the file
/// that holds its meshes, or a mesh alone.
#[derive(Debug, Clone, Copy)]
enum RenderObject<'a> {
    Hlod {
        hlod: &'a Hlod,
        hierarchy: &'a Hierarchy,
        file: &'a W3dFile,
    },
    Mesh(&'a W3dMesh),
}

impl ModelImport {
    /// The model `name`, a render object's name as an object writes it: `NULL`, or a render
    /// object of that name in the file of the name's part before its first `.`, its HLOD's
    /// hierarchy in that file or in the file of its own name. Its files and its textures are
    /// found through `assets` first, and it converts from them alone.
    pub(crate) fn convert(
        name: &str,
        assets: &mut ModelAssets<'_>,
    ) -> Result<Converted, ZeroHourError> {
        let name = name.to_ascii_lowercase();
        if name == NULL {
            return Ok(Converted::Nothing);
        }
        let stem = name
            .split('.')
            .next()
            .expect("split gives one part at least");
        assets.load(stem)?;
        let other = assets.file(stem).and_then(|file| {
            let hlod = file.hlods.iter().find(|hlod| hlod.name == name)?;
            let own = file.hierarchy(&hlod.hierarchy).is_some();
            (!own).then(|| hlod.hierarchy.clone())
        });
        if let Some(other) = &other {
            assets.load(other)?;
        }
        let error = |error| ZeroHourError::Model {
            model: name.clone(),
            error,
        };
        let Some(file) = assets.file(stem) else {
            return Ok(Converted::Missing);
        };
        let object = if let Some(hlod) = file.hlods.iter().find(|hlod| hlod.name == name) {
            let hierarchy = file
                .hierarchy(&hlod.hierarchy)
                .or_else(|| assets.file(&hlod.hierarchy)?.hierarchy(&hlod.hierarchy))
                .ok_or_else(|| error(ModelError::NoHierarchy(hlod.hierarchy.clone())))?;
            RenderObject::Hlod {
                hlod,
                hierarchy,
                file,
            }
        } else if let Some(mesh) = file.meshes.iter().find(|mesh| mesh.name == name) {
            RenderObject::Mesh(mesh)
        } else if file.boxes.contains(&name) {
            return Ok(Converted::Nothing);
        } else if file.emitters.contains(&name) {
            return Ok(Converted::Emitter);
        } else {
            return Ok(Converted::Missing);
        };
        let textures = ModelTextures::of(object.meshes(), assets);
        let model = Self::of(&name, object, &textures).map_err(error)?;
        Ok(Converted::Model(Box::new(model)))
    }

    /// The model `name` of `object`, the files of the textures its meshes name `textures`.
    fn of(
        name: &str,
        object: RenderObject<'_>,
        textures: &ModelTextures,
    ) -> Result<Self, ModelError> {
        let mut builder = ModelBuilder::new(name);
        let (nodes, aggregates) = match object {
            RenderObject::Hlod {
                hlod,
                hierarchy,
                file,
            } => {
                let (sub_objects, meshes): (Vec<_>, Vec<_>) =
                    RenderObject::drawn(hlod, file).unzip();
                let nodes = builder.hlod(hierarchy, &sub_objects, &meshes, textures)?;
                let aggregates = hlod.aggregates.iter();
                (
                    nodes,
                    aggregates.map(|aggregate| aggregate.name.clone()).collect(),
                )
            }
            RenderObject::Mesh(mesh) => (builder.mesh_alone(mesh, textures)?, Vec::new()),
        };
        Ok(Self {
            glb: builder.glb,
            nodes,
            missing_textures: builder.missing_textures,
            aggregates,
        })
    }
}

impl<'a> RenderObject<'a> {
    /// The meshes it draws.
    fn meshes(self) -> impl Iterator<Item = &'a W3dMesh> {
        let (hlod, mesh) = match self {
            Self::Hlod { hlod, file, .. } => (Some((hlod, file)), None),
            Self::Mesh(mesh) => (None, Some(mesh)),
        };
        hlod.into_iter()
            .flat_map(|(hlod, file)| Self::drawn(hlod, file).filter_map(|(_, mesh)| mesh))
            .chain(mesh)
    }

    /// The sub-objects of `hlod` that `file` holds a render object of, each with its mesh, none
    /// for a box: `HLodClass` skips a sub-object no render object has, and keeps a box, which the
    /// game never draws, as no code sets `Set_Box_Display_Mask`.
    fn drawn(
        hlod: &'a Hlod,
        file: &'a W3dFile,
    ) -> impl Iterator<Item = (&'a SubObject, Option<&'a W3dMesh>)> {
        hlod.sub_objects.iter().filter_map(move |sub| {
            let mesh = file.meshes.iter().find(|mesh| mesh.name == sub.name);
            (mesh.is_some() || file.boxes.contains(&sub.name)).then_some((sub, mesh))
        })
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::zero_hour::format::w3d::hierarchy::{Hierarchy, Pivot};
    use crate::zero_hour::format::w3d::hlod::{Hlod, SubObject};
    use crate::zero_hour::format::w3d::w3d_file::internals::{box_chunk, emitter_chunk};
    use crate::zero_hour::format::w3d::w3d_mesh::internals::SKIN_GEOMETRY;
    use crate::zero_hour::format::w3d::w3d_mesh::{
        Ids, Pass, Shader, Stage, TWO_SIDED, Texture, VertexMaterial, W3dMesh,
    };

    /// `Tank.w3d`: the HLOD `TANK` on its hierarchy of a root, a turret a half turn about `z` at
    /// `(1, 2, 3)`, and a flash bone at `(0, 1, 0)` on the turret. Its sub-objects: a hull of
    /// vertex colors, coordinates of `NaN` and infinity at one vertex, and two triangles, one of a
    /// clamped texture `Rock.tga` and one of none; on the turret a triangle with zero normals and a
    /// vertex no triangle uses; on the flash bone an additive two-sided triangle of a texture no
    /// archive holds, `Gone.tga`, and no coordinates; a box; a name of no render object; and a skin
    /// on the turret.
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
            passes: vec![Pass {
                materials: Ids::One(0),
                shaders: Ids::One(0),
                colors: Some(vec![[10, 20, 30, 40]; 4]),
                stage: Some(Stage {
                    textures: Ids::Each(vec![0, Stage::NONE]),
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
            passes: vec![Pass {
                materials: Ids::One(0),
                shaders: Ids::One(0),
                colors: None,
                stage: Some(Stage {
                    textures: Ids::One(0),
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
            bones: Some(vec![1; 3]),
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
}

#[cfg(test)]
mod tests;
