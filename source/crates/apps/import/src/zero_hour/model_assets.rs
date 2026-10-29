use std::collections::BTreeMap;

use crate::zero_hour::art_paths::ArtPaths;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::w3d::w3d_animation::W3dAnimation;
use crate::zero_hour::format::w3d::w3d_file::W3dFile;
use crate::zero_hour::install::Install;

/// The install's W3D files, each read once, found as `ArtPaths` finds a model.
#[derive(Debug)]
pub(crate) struct ModelAssets<'a> {
    install: &'a Install,
    art: ArtPaths<'a>,
    files: BTreeMap<String, Option<W3dFile>>,
}

impl<'a> ModelAssets<'a> {
    pub(crate) const fn new(install: &'a Install) -> Self {
        ModelAssets {
            install,
            art: ArtPaths::new(install),
            files: BTreeMap::new(),
        }
    }

    /// Where the install's art lies.
    pub(crate) const fn art(&self) -> ArtPaths<'a> {
        self.art
    }

    /// Reads the W3D file `<stem>.w3d` once, if the install holds one, for `file` to give.
    pub(crate) fn load(&mut self, stem: &str) -> Result<(), ZeroHourError> {
        let stem = stem.to_ascii_lowercase();
        if !self.files.contains_key(&stem) {
            let read = match self.art.find("w3d", &format!("{stem}.w3d")) {
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

    /// The animation `name` of the W3D file `<stem>.w3d` that `load` read, which the file no
    /// longer holds, as each animation converts once; none for a file the install lacks, one not
    /// loaded, or one of no animation of that name.
    pub(crate) fn take_animation(&mut self, stem: &str, name: &str) -> Option<W3dAnimation> {
        self.files
            .get_mut(&stem.to_ascii_lowercase())?
            .as_mut()?
            .take_animation(name)
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
        let assets = ModelAssets::new(&install);
        let texture = |name: &str| assets.art().texture(name).map(|path| path.to_string());
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
