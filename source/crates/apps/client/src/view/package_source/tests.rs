use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::asset::io::{AssetSourceBuilder, AssetSourceId};
use bevy::asset::{
    Asset, AssetApp, AssetLoader, AssetServer, Handle, LoadContext, LoadState, RenderAssetUsages,
};
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::reflect::TypePath;
use bevy::render::render_resource::TextureFormat;
use bevy::tasks::{block_on, poll_once};
use campfire_store::InputFile;
use campfire_store::internals::TestDir;

use super::*;
use crate::test_app;
use crate::view::package_source::samples::package;

#[test]
fn a_path_reads_its_packages_file_checked_against_its_index() {
    let test_dir = TestDir::new();
    let mode = package(
        &test_dir,
        "mode",
        &[("client/models/tank.glb", b"glTF tank")],
    );
    let other = package(
        &test_dir,
        "other",
        &[("client/models/tank.glb", b"glTF other")],
    );
    let source = PackageAssets::new(vec![mode, other]);
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
    test_dir.write("mode/client/models/tank.glb", b"glTF tamk");
    assert!(matches!(
        read("0/client/models/tank.glb"),
        Err(AssetReaderError::Io(_))
    ));
    // The asset path of a package's file names the source.
    let path = PackagePath::parse("client/models/tank.glb").unwrap();
    assert_eq!(
        PackageAssets::asset_path(PackageIndex::new(1), &path),
        "package://1/client/models/tank.glb"
    );
}

#[test]
fn bevys_ktx2_loader_reads_the_imports_textures_with_their_levels() {
    // The DDS's 8 × 8 DXT1 blocks and its 4 levels, 32 + 8 + 8 + 8 bytes; the TGA's 2 × 1
    // texels as RGBA8 and its 2 levels, 8 + 4 bytes; both sRGB.
    // The import's test pins them.
    let fixture = |name: &str| {
        let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
        InputFile::read(&fixtures.join(name), 1 << 10).unwrap()
    };
    let (dxt1, rgba) = (fixture("dxt1.ktx2"), fixture("rgba.ktx2"));
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

/// Five models and a texture, of one package.
fn models(test_dir: &TestDir) -> PackageAssets {
    let files: Vec<(String, &[u8])> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|name| (format!("client/models/{name}.glb"), &b"glTF"[..]))
        .chain([("client/textures/sand.ktx2".to_owned(), &b"KTX"[..])])
        .collect();
    let files: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), *bytes))
        .collect();
    PackageAssets::new(vec![package(test_dir, "mode", &files)])
}

#[test]
fn at_most_four_models_read_at_once_and_a_texture_waits_for_none() {
    let test_dir = TestDir::new();
    let source = models(&test_dir);
    let paths: Vec<PathBuf> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|name| PathBuf::from(format!("0/client/models/{name}.glb")))
        .collect();
    let mut held: Vec<_> = paths[..4]
        .iter()
        .map(|path| block_on(source.read(path)).unwrap())
        .collect();
    assert_eq!(held.len(), 4);
    // A fifth model waits while four readers hold their permits; a texture does not.
    let mut fifth = pin!(source.read(&paths[4]));
    assert!(block_on(poll_once(fifth.as_mut())).is_none());
    let texture = pin!(source.read(Path::new("0/client/textures/sand.ktx2")));
    assert!(block_on(poll_once(texture)).is_some());
    // Dropping one reader lets the fifth read.
    held.pop();
    let read = block_on(poll_once(fifth.as_mut())).expect("a permit is free");
    assert!(read.is_ok());
}

/// A model loader that reads its file, counts itself as started, and waits for a pass of `gate`
/// before it ends.
#[derive(Debug, TypePath)]
struct GatedLoader {
    started: Arc<AtomicUsize>,
    gate: Arc<Semaphore>,
}

#[derive(Debug, Asset, TypePath)]
struct Model;

impl AssetLoader for GatedLoader {
    type Asset = Model;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        (): &(),
        _: &mut LoadContext<'_>,
    ) -> Result<Model, io::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        self.started.fetch_add(1, Ordering::SeqCst);
        let _pass = self.gate.acquire().await;
        Ok(Model)
    }

    fn extensions(&self) -> &[&str] {
        &["glb"]
    }
}

#[test]
fn bevy_keeps_a_models_reader_and_its_permit_for_the_whole_load() {
    let test_dir = TestDir::new();
    let source = models(&test_dir);
    let permits = Arc::clone(&source.models);
    let started = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Semaphore::new(0));
    let mut app = test_app::assets_with(
        AssetSourceId::from(PackageAssets::NAME),
        AssetSourceBuilder::new(move || Box::new(source.clone())),
    );
    app.init_asset::<Model>();
    app.register_asset_loader(GatedLoader {
        started: Arc::clone(&started),
        gate: Arc::clone(&gate),
    });
    app.finish();
    app.cleanup();
    let server = app.world().resource::<AssetServer>().clone();
    let path = |name: &str| format!("package://0/client/models/{name}.glb");
    let handles: Vec<Handle<Model>> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|name| server.load(path(name)))
        .collect();
    let mut updates = 0;
    while started.load(Ordering::SeqCst) < 4 {
        app.update();
        updates += 1;
        assert!(updates < 100_000, "four loads start");
    }
    // Four loads wait in their loaders after reading: their readers hold every permit, so the
    // fifth has not started, and no permit is free.
    for _ in 0..100 {
        app.update();
    }
    assert_eq!(started.load(Ordering::SeqCst), 4);
    assert!(permits.try_acquire_arc().is_none());
    // Let every loader end: all five load, and every permit comes back.
    gate.add_permits(5);
    let mut updates = 0;
    while !handles
        .iter()
        .all(|handle| matches!(server.load_state(handle), LoadState::Loaded))
    {
        app.update();
        updates += 1;
        assert!(updates < 100_000, "five loads end");
    }
    assert_eq!(started.load(Ordering::SeqCst), 5);
    #[expect(
        clippy::needless_collect,
        reason = "each permit stays held as the next is taken, so four free count as four"
    )]
    let free: Vec<_> = (0..4).filter_map(|_| permits.try_acquire_arc()).collect();
    assert_eq!(free.len(), 4);
}
