//! Models into glTF 2.0's binary form, as any import writes them: a JSON document through
//! `common`'s `codec`, and its buffer in the GLB container, written by hand.

pub(crate) mod glb_builder;
pub(crate) mod gltf_document;
