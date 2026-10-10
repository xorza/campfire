use std::future::ready;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetReaderFuture, PathStream, Reader, VecReader,
};
use bevy::tasks::ConditionalSendFuture;
use campfire_capabilities::PackagePath;
use campfire_log::ErrorReport;
use campfire_package::{ContentError, ModePackages, PackageReader};
use tracing::error;

/// The mode's packages as a Bevy asset source: a path is a package's place among them, the
/// mode's 0, then its path in that package, as `0/client/models/tank.glb`, so a model's textures
/// resolve by their paths beside it. Each read goes through the package's reader, which checks
/// the bytes against the package's index; Bevy runs it on its IO pool, not the frame's thread.
#[derive(Debug)]
pub(crate) struct PackageSource {
    /// By place.
    packages: Vec<PackageReader>,
}

impl PackageSource {
    /// The source's name, which an asset path of it names.
    pub(crate) const NAME: &'static str = "package";

    pub(crate) fn of(packages: &ModePackages) -> PackageSource {
        PackageSource {
            packages: packages
                .packages()
                .map(|view| view.package.files.clone())
                .collect(),
        }
    }

    /// The asset path of the file at `path` in the package at `place`.
    pub(crate) fn asset_path(place: u16, path: &PackagePath) -> String {
        format!("{}://{place}/{}", PackageSource::NAME, path.as_str())
    }

    /// The bytes of the file at `path`, a package's place then its path there; not found for a
    /// path of no package or of no file.
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, AssetReaderError> {
        let missing = || AssetReaderError::NotFound(path.to_owned());
        let mut names = path.components().map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        });
        let place = names
            .next()
            .flatten()
            .and_then(|place| place.parse::<usize>().ok());
        let package = place
            .and_then(|place| self.packages.get(place))
            .ok_or_else(missing)?;
        let rest: Option<Vec<&str>> = names.collect();
        let file = rest
            .and_then(|names| PackagePath::parse(&names.join("/")))
            .ok_or_else(missing)?;
        package.read_file(&file).map_err(|problem| match problem {
            ContentError::Missing { .. } => missing(),
            problem => {
                error!(path = %path.display(), error = %ErrorReport::of(&problem), "a package's asset does not read");
                AssetReaderError::Io(Arc::new(io::Error::other(problem)))
            }
        })
    }
}

impl AssetReader for PackageSource {
    fn read<'a>(&'a self, path: &'a Path) -> impl AssetReaderFuture<Value: Reader + 'a> {
        ready(self.read_bytes(path).map(VecReader::new))
    }

    // A package holds no Bevy meta files: every asset loads by its loader's defaults.
    fn read_meta<'a>(&'a self, path: &'a Path) -> impl AssetReaderFuture<Value: Reader + 'a> {
        ready(Err::<VecReader, _>(AssetReaderError::NotFound(
            path.to_owned(),
        )))
    }

    fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<PathStream>, AssetReaderError>> {
        ready(Err(AssetReaderError::NotFound(PathBuf::from(path))))
    }

    fn is_directory<'a>(
        &'a self,
        _: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<bool, AssetReaderError>> {
        ready(Ok(false))
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_capabilities::PackagePath;
    use campfire_package::{PackageDir, PackageReader, PackageWriter};
    use campfire_store::Scratch;

    /// The package `dir` of `scratch`, written with `files`, as its client reads it.
    pub(crate) fn package(scratch: &Scratch, dir: &str, files: &[(&str, &[u8])]) -> PackageReader {
        let mut writer = PackageWriter::create(&scratch.path(dir)).unwrap();
        for (path, bytes) in files {
            writer
                .write(PackagePath::parse(path).unwrap(), bytes)
                .unwrap();
        }
        writer.finish().unwrap();
        PackageDir::new(scratch.path(dir))
            .read()
            .unwrap()
            .reader()
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
    use bevy::render::render_resource::TextureFormat;
    use bevy::tasks::block_on;
    use campfire_import::internals as import;
    use campfire_store::Scratch;

    use super::*;
    use crate::view::package_source::internals::package;

    #[test]
    fn a_path_reads_its_packages_file_checked_against_its_index() {
        let scratch = Scratch::new();
        let mode = package(
            &scratch,
            "mode",
            &[("client/models/tank.glb", b"glTF tank")],
        );
        let other = package(
            &scratch,
            "other",
            &[("client/models/tank.glb", b"glTF other")],
        );
        let source = PackageSource {
            packages: vec![mode, other],
        };
        let read = |path: &str| {
            block_on(async {
                let mut reader = source.read(Path::new(path)).await?;
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes).await.unwrap();
                Ok::<_, AssetReaderError>(bytes)
            })
        };
        // Each package's file by its place, the mode's 0.
        assert_eq!(read("0/client/models/tank.glb").unwrap(), b"glTF tank");
        assert_eq!(read("1/client/models/tank.glb").unwrap(), b"glTF other");
        // A place of no package, a path of no file, and a path no package holds are not found.
        for path in [
            "2/client/models/tank.glb",
            "0/client/models/jeep.glb",
            "x/a",
            "0/../a",
        ] {
            assert!(
                matches!(read(path), Err(AssetReaderError::NotFound(_))),
                "{path}"
            );
        }
        // A file whose bytes are not its row's does not read.
        scratch.write("mode/client/models/tank.glb", b"glTF tamk");
        assert!(matches!(
            read("0/client/models/tank.glb"),
            Err(AssetReaderError::Io(_))
        ));
        // The asset path of a package's file names the source.
        let path = PackagePath::parse("client/models/tank.glb").unwrap();
        assert_eq!(
            PackageSource::asset_path(1, &path),
            "package://1/client/models/tank.glb"
        );
    }

    #[test]
    fn bevys_ktx2_loader_reads_the_imports_textures_with_their_levels() {
        // The DDS's 8 × 8 DXT1 blocks and its 4 levels, 32 + 8 + 8 + 8 bytes; the TGA's 2 × 1
        // texels as RGBA8 and its 2 levels, 8 + 4 bytes; both sRGB.
        let [dxt1, rgba] = import::ktx2_fixtures();
        let read = |bytes: &[u8]| {
            let image = Image::from_buffer(
                bytes,
                ImageType::Extension("ktx2"),
                CompressedImageFormats::BC,
                true,
                ImageSampler::Default,
                RenderAssetUsages::default(),
            )
            .unwrap();
            let descriptor = &image.texture_descriptor;
            let size = descriptor.size;
            (
                descriptor.format,
                [size.width, size.height],
                descriptor.mip_level_count,
                image.data.map(|data| data.len()),
            )
        };
        assert_eq!(
            read(&dxt1),
            (TextureFormat::Bc1RgbaUnormSrgb, [8, 8], 4, Some(56))
        );
        assert_eq!(
            read(&rgba),
            (TextureFormat::Rgba8UnormSrgb, [2, 1], 2, Some(12))
        );
    }
}
