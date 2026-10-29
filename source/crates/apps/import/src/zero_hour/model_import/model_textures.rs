use std::collections::BTreeMap;

use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::format::w3d::w3d_mesh::W3dMesh;
use crate::zero_hour::model_assets::ModelAssets;

/// The files of the textures a model's meshes name, as the install finds them, by the name a
/// mesh gives each; none for a name no archive holds, which the game draws as `MissingTexture`.
#[derive(Debug, Default)]
pub(super) struct ModelTextures(BTreeMap<String, Option<ArchivePath>>);

impl ModelTextures {
    /// The files of every texture `meshes` name, as `assets` finds them.
    pub(super) fn of<'m>(
        meshes: impl IntoIterator<Item = &'m W3dMesh>,
        assets: &ModelAssets<'_>,
    ) -> Self {
        meshes
            .into_iter()
            .flat_map(|mesh| &mesh.textures)
            .map(|texture| (texture.name.clone(), assets.art().texture(&texture.name)))
            .collect()
    }

    /// The file of the texture a mesh names `name`; none for one no archive holds.
    pub(super) fn get(&self, name: &str) -> Option<&ArchivePath> {
        self.0.get(name)?.as_ref()
    }
}

impl FromIterator<(String, Option<ArchivePath>)> for ModelTextures {
    fn from_iter<I: IntoIterator<Item = (String, Option<ArchivePath>)>>(textures: I) -> Self {
        Self(textures.into_iter().collect())
    }
}
