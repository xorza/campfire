use super::*;

#[test]
fn each_drawn_texture_goes_once_and_two_that_share_a_file_clash() {
    // A texture drawn twice goes once; a DDS and a TGA of one name in one directory both go to
    // `x.ktx2`, which the import refuses, naming both.
    let path = ArchivePath::of;
    let files =
        GameWrite::texture_files([path("art\\a.dds"), path("art\\b.tga"), path("art\\a.dds")])
            .unwrap();
    let at = |text: &str| PackagePath::parse(text).unwrap();
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        [
            (at("client/textures/art/a.ktx2"), path("art\\a.dds")),
            (at("client/textures/art/b.ktx2"), path("art\\b.tga")),
        ]
    );
    let clash = GameWrite::texture_files([path("art\\x.dds"), path("art\\x.tga")]).unwrap_err();
    assert!(matches!(
        clash,
        ZeroHourError::TextureClash { first, second }
            if first == path("art\\x.dds") && second == path("art\\x.tga")
    ));
}
