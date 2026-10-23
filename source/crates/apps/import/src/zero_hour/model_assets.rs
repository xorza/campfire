use std::collections::BTreeMap;

use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::w3d::w3d_file::W3dFile;
use crate::zero_hour::install::Install;

/// The install's models and textures, found as `GameFileClass::Set_Name` finds them: in the
/// language's directory, `Data\<language>\Art\...`, then in `Art\...`. Each W3D file is read once.
#[derive(Debug)]
pub(crate) struct ModelAssets<'a> {
    install: &'a Install,
    language: Option<String>,
    files: BTreeMap<String, Option<W3dFile>>,
}

impl<'a> ModelAssets<'a> {
    pub(crate) fn new(install: &'a Install) -> Result<Self, ZeroHourError> {
        let language = install.language()?;
        Ok(ModelAssets {
            install,
            language,
            files: BTreeMap::new(),
        })
    }

    /// Reads the W3D file `<stem>.w3d` once, if the install holds one, for `file` to give.
    pub(crate) fn load(&mut self, stem: &str) -> Result<(), ZeroHourError> {
        let stem = stem.to_ascii_lowercase();
        if !self.files.contains_key(&stem) {
            let read = match self.find("w3d", &format!("{stem}.w3d")) {
                Some(path) => {
                    let bytes = self.install.read(&path)?;
                    let file = W3dFile::read(&bytes).map_err(|error| ZeroHourError::W3d {
                        path: path.clone(),
                        error,
                    })?;
                    Some(file)
                }
                None => None,
            };
            self.files.insert(stem, read);
        }
        Ok(())
    }

    /// The W3D file `<stem>.w3d` that `load` read; none for one the install lacks, or one not
    /// loaded.
    pub(crate) fn file(&self, stem: &str) -> Option<&W3dFile> {
        self.files.get(&stem.to_ascii_lowercase())?.as_ref()
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

    /// The file `name` in `Data\<language>\Art\<art_dir>\`, else in `Art\<art_dir>\`.
    fn find(&self, art_dir: &str, name: &str) -> Option<ArchivePath> {
        let localized = self
            .language
            .as_ref()
            .map(|language| ArchivePath::of(&format!("data\\{language}\\art\\{art_dir}\\{name}")));
        localized
            .into_iter()
            .chain([ArchivePath::of(&format!("art\\{art_dir}\\{name}"))])
            .find(|path| self.install.contains(path))
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::zero_hour::install::samples::FixtureInstall;

    #[test]
    fn a_texture_is_its_dds_else_its_tga_as_named() {
        let FixtureInstall {
            test_dir: _dir,
            install,
        } = FixtureInstall::new();
        let assets = ModelAssets::new(&install).unwrap();
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
