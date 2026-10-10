use std::collections::BTreeMap;

use crate::error::ImportError;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::install::Install;
use crate::zero_hour::w3d_file::W3dFile;

/// The install's models and textures, found as `GameFileClass::Set_Name` finds them: in the
/// language's folder, `Data\<language>\Art\...`, then in `Art\...`. Each W3D file is read once.
#[derive(Debug)]
pub(crate) struct ModelAssets<'a> {
    install: &'a mut Install,
    language: Option<String>,
    files: BTreeMap<String, Option<W3dFile>>,
}

impl<'a> ModelAssets<'a> {
    pub(crate) fn new(install: &'a mut Install) -> Result<ModelAssets<'a>, ZeroHourError> {
        let language = install.language()?;
        Ok(ModelAssets {
            install,
            language,
            files: BTreeMap::new(),
        })
    }

    /// The W3D file `<stem>.w3d`, if the install holds one.
    pub(crate) fn file(&mut self, stem: &str) -> Result<Option<&W3dFile>, ImportError> {
        let stem = stem.to_ascii_lowercase();
        if !self.files.contains_key(&stem) {
            let read = match self.find("w3d", &format!("{stem}.w3d")) {
                Some(path) => {
                    let bytes = self.install.read(&path)?;
                    let file = W3dFile::read(&bytes).map_err(|error| {
                        ImportError::ZeroHour(ZeroHourError::W3d {
                            path: path.to_string(),
                            error,
                        })
                    })?;
                    Some(file)
                }
                None => None,
            };
            self.files.insert(stem.clone(), read);
        }
        Ok(self.files[&stem].as_ref())
    }

    /// The texture a mesh names as `name`, as the loader finds it: `DDSFileClass`'s name, the
    /// last three letters made `dds`; else the name itself, which the TGA reader reads, so only
    /// a `.tga` the importer converts.
    pub(crate) fn texture(&self, name: &str) -> Option<ArchivePath> {
        let named = ArchivePath::of(name);
        let stem = named.as_str().get(..named.as_str().len().checked_sub(3)?)?;
        self.find("textures", &format!("{stem}dds")).or_else(|| {
            (named.extension() == Some("tga"))
                .then(|| self.find("textures", named.as_str()))
                .flatten()
        })
    }

    /// The file `name` in `Data\<language>\Art\<folder>\`, else in `Art\<folder>\`.
    fn find(&self, folder: &str, name: &str) -> Option<ArchivePath> {
        let localized = self
            .language
            .as_ref()
            .map(|language| ArchivePath::of(&format!("data\\{language}\\art\\{folder}\\{name}")));
        localized
            .into_iter()
            .chain([ArchivePath::of(&format!("art\\{folder}\\{name}"))])
            .find(|path| self.install.contains(path))
    }
}

#[cfg(test)]
mod tests {
    use campfire_store::Scratch;

    use super::*;
    use crate::zero_hour::install::internals::fixture;

    #[test]
    fn a_texture_is_its_dds_else_its_tga_as_named() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let mut install = Install::open(&scratch.path("zh")).unwrap();
        let assets = ModelAssets::new(&mut install).unwrap();
        let texture = |name: &str| assets.texture(name).map(|path| path.to_string());
        // The archives hold `Rock.dds` and `Sign.tga`.
        let rock = Some("art\\textures\\rock.dds".to_owned());
        assert_eq!(texture("Rock.tga"), rock);
        assert_eq!(texture("rock.bmp"), rock);
        assert_eq!(
            texture("SIGN.TGA"),
            Some("art\\textures\\sign.tga".to_owned())
        );
        // `Sign.bmp` is read as `Sign.dds`, then as itself, which no archive holds.
        assert_eq!(texture("Sign.bmp"), None);
        assert_eq!(texture("ab"), None);
    }
}
