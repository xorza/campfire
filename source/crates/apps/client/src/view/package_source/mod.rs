use std::fmt;
use std::future::ready;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_lock::{Semaphore, SemaphoreGuardArc};
use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetReaderFuture, PathStream, Reader, ReaderNotSeekableError,
    STACK_FUTURE_SIZE, SeekableReader, StackFuture, VecReader,
};
use bevy::tasks::ConditionalSendFuture;
use bevy::tasks::futures_lite::AsyncRead;
use campfire_log::ErrorReport;
use campfire_package::{ContentError, ModePackages, PackageReader};
use campfire_values::{PackageIndex, PackagePath};
use tracing::error;

/// The mode's packages as a Bevy asset source: a path is a package's place among them, the
/// mode's 0, then its path in that package, as `0/client/models/tank.glb`, so a model's textures
/// resolve by their paths beside it. Each read goes through the package's reader, which checks
/// the bytes against the package's index; Bevy runs it on its IO pool, not the frame's thread.
///
/// At most `MODELS_AT_ONCE` glTF models load at a time (design 17, D9): a model's read waits for
/// a permit, which its reader holds until the asset server drops it after the load. Bevy's glTF
/// loader waits on a task scope, and its thread runs the next queued load inside that wait, so
/// with no bound the loads nest as deep as there are models (Bevy #21954); a load that waits
/// for a permit yields instead. No model's load waits on another model's, which would hold every
/// permit and wait forever once four did: Bevy's glTF loader loads its textures apart and reads
/// a `.gltf`'s buffers, which are no models.
#[derive(Debug, Clone)]
pub(crate) struct PackageSource {
    /// By place.
    packages: Vec<PackageReader>,
    models: Arc<Semaphore>,
}

/// A file's bytes, and the permit of a model's load, which ends as the asset server drops it.
struct HeldReader {
    bytes: VecReader,
    permit: Option<SemaphoreGuardArc>,
}

/// The models that load at once.
const MODELS_AT_ONCE: usize = 4;

impl PackageSource {
    /// The source's name, which an asset path of it names.
    pub(crate) const NAME: &'static str = "package";

    pub(crate) fn of(packages: &ModePackages) -> PackageSource {
        PackageSource::new(
            packages
                .packages()
                .map(|view| view.package.files.clone())
                .collect(),
        )
    }

    fn new(packages: Vec<PackageReader>) -> PackageSource {
        PackageSource {
            packages,
            models: Arc::new(Semaphore::new(MODELS_AT_ONCE)),
        }
    }

    /// Whether the file at `path` is a glTF model, whose load holds a permit.
    fn is_model(path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| extension == "glb" || extension == "gltf")
    }

    /// The asset path of the file at `path` in the package at `place`.
    pub(crate) fn asset_path(place: PackageIndex, path: &PackagePath) -> String {
        let place = place.get();
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
        async move {
            let permit = if PackageSource::is_model(path) {
                Some(self.models.acquire_arc().await)
            } else {
                None
            };
            Ok(HeldReader {
                bytes: VecReader::new(self.read_bytes(path)?),
                permit,
            })
        }
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

impl AsyncRead for HeldReader {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().bytes).poll_read(context, buf)
    }
}

impl Reader for HeldReader {
    fn read_to_end<'a>(
        &'a mut self,
        buf: &'a mut Vec<u8>,
    ) -> StackFuture<'a, io::Result<usize>, STACK_FUTURE_SIZE> {
        self.bytes.read_to_end(buf)
    }

    fn seekable(&mut self) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError> {
        self.bytes.seekable()
    }
}

impl fmt::Debug for HeldReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HeldReader")
            .field("bytes", &self.bytes.bytes.len())
            .field("permit", &self.permit.is_some())
            .finish()
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_package::PackageReader;
    use campfire_package_disk::{PackageDir, PackageWriter};
    use campfire_store::internals::Scratch;
    use campfire_values::PackagePath;

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
            .files()
            .unwrap()
            .reader()
            .clone()
    }
}

#[cfg(test)]
mod tests;
