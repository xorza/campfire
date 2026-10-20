//! Models into glTF 2.0's binary form, as any import writes them: a JSON document of `gltf-json`'s
//! schema through `campfire-codec`, and its buffer in the GLB container, written by hand.

pub(crate) mod glb_builder;
pub(crate) mod model_bounds;
