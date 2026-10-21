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
pub(crate) mod samples;

#[cfg(test)]
mod tests;
