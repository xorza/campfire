use bevy::asset::{AssetPath, AssetServer, Handle, RenderAssetUsages};
use bevy::ecs::resource::Resource;
use bevy::gltf::GltfLoaderSettings;
use bevy::platform::collections::HashMap;
use bevy::world_serialization::WorldAsset;

/// The scene of each model the client drew, by its path, each loaded once and kept for the match,
/// its meshes kept in the main world as well as on the GPU, as the pointer casts its ray at their
/// triangles (02-engine-core.md, A unit is picked by its drawn triangles). A look's rest models and
/// a client object's load as the match starts, and a state's other models as a unit first takes
/// it, which show once they are ready (animated-looks.md, A state's new models replace the old as
/// they are ready).
#[derive(Resource, Debug, Default)]
pub(crate) struct ModelScenes {
    scenes: HashMap<AssetPath<'static>, Handle<WorldAsset>>,
}

impl ModelScenes {
    /// The scene at `path`, which loads the first time it is asked for.
    pub(crate) fn scene(
        &mut self,
        path: &AssetPath<'static>,
        assets: &AssetServer,
    ) -> Handle<WorldAsset> {
        if let Some(scene) = self.scenes.get(path) {
            return scene.clone();
        }
        let scene = assets
            .load_builder()
            .with_settings(|settings: &mut GltfLoaderSettings| {
                settings.load_meshes = RenderAssetUsages::all();
            })
            .load(path.clone());
        self.scenes.insert(path.clone(), scene.clone());
        scene
    }
}

#[cfg(test)]
pub(crate) mod reach_in {
    use super::*;

    impl ModelScenes {
        /// How many scenes it loaded.
        pub(crate) fn count(&self) -> usize {
            self.scenes.len()
        }
    }
}
