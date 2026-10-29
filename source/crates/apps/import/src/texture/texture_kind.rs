/// A texture's file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextureKind {
    Dds,
    Tga,
}

impl TextureKind {
    /// The format of a texture file of `extension`, in lowercase; none for a file of another kind.
    pub(crate) fn of(extension: &str) -> Option<Self> {
        match extension {
            "dds" => Some(Self::Dds),
            "tga" => Some(Self::Tga),
            _ => None,
        }
    }
}
