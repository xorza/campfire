//! Textures into KTX2, as any import writes them: a DDS's DXT blocks as they are, and a TGA's
//! texels as 8-bit RGBA with levels computed in linear light, every one sRGB.

pub(crate) mod dds_file;
pub(crate) mod error;
pub(crate) mod ktx2_image;
pub(crate) mod mip_chain;
pub(crate) mod srgb;
pub(crate) mod texture_format;
pub(crate) mod tga_file;
