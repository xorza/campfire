use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::install::Install;

/// Where the install's art lies, as `GameFileClass::Set_Name` finds a model or a texture: in the
/// language's directory, `Data\<language>\Art\...`, then in `Art\...`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArtPaths<'a> {
    install: &'a Install,
}

impl<'a> ArtPaths<'a> {
    pub(crate) const fn new(install: &'a Install) -> Self {
        Self { install }
    }

    /// The texture a mesh names as `name`, as the loader finds it: `DDSFileClass`'s name, the
    /// last three letters made `dds`; else the name itself, which the TGA reader reads, so only
    /// a `.tga` the importer converts.
    pub(crate) fn texture(self, name: &str) -> Option<ArchivePath> {
        let named = ArchivePath::of(name);
        let stem = named.as_str().get(..named.as_str().len().checked_sub(3)?)?;
        self.find("textures", &format!("{stem}dds")).or_else(|| {
            (named.extension() == Some("tga"))
                .then(|| self.find("textures", named.as_str()))
                .flatten()
        })
    }

    /// The file `name` in `Data\<language>\Art\<art_dir>\`, else in `Art\<art_dir>\`.
    pub(crate) fn find(self, art_dir: &str, name: &str) -> Option<ArchivePath> {
        let localized = self
            .install
            .language()
            .map(|language| ArchivePath::of(&format!("data\\{language}\\art\\{art_dir}\\{name}")));
        localized
            .into_iter()
            .chain([ArchivePath::of(&format!("art\\{art_dir}\\{name}"))])
            .find(|path| self.install.contains(path))
    }
}
