use super::*;

#[test]
fn a_type_takes_its_texture_the_default_first_and_a_later_file_last() {
    let mut ini = TerrainIni::default();
    let default = b"Terrain DefaultTerrain\n  Texture = NoTexture.tga\n  Class = NONE\nEnd\n";
    ini.read(default).unwrap();
    let types = b"Terrain Sand\n  Texture = TSand.tga\nEnd\nTerrain Bare\n  Class = DESERT\nEnd\n";
    ini.read(types).unwrap();
    ini.read(b"terrain Sand\n  texture = TSand2.tga\nEnd\n")
        .unwrap();
    // A later file's texture wins; a type with none keeps the default's; names match
    // exactly, as the game compares them.
    assert_eq!(ini.texture("Sand"), Some("TSand2.tga"));
    assert_eq!(ini.texture("Bare"), Some("NoTexture.tga"));
    assert_eq!(ini.texture("sand"), None);

    let fails = |text: &[u8]| TerrainIni::default().read(text).unwrap_err();
    assert_eq!(fails(b"Texture = A.tga\n"), IniError::NotBlock { line: 1 });
    assert_eq!(fails(b"End\n"), IniError::StrayEnd { line: 1 });
    assert_eq!(fails(b"Terrain\nEnd\n"), IniError::NoName { line: 1 });
    assert_eq!(
        fails(b"Terrain A\n Texture\nEnd\n"),
        IniError::NoValue { line: 2 }
    );
    assert_eq!(fails(b"Terrain A\n"), IniError::Unclosed);
}
