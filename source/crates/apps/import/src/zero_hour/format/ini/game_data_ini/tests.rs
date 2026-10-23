use super::*;

#[test]
fn a_field_keeps_the_default_until_a_file_sets_it_and_the_last_file_wins() {
    // The terrain's fields: `AdjustCliffTextures` off and three-way blends on by default, and
    // the blends on for any count but 0.
    let mut ini = GameDataIni::default();
    let fields = |ini: &GameDataIni| {
        (
            ini.terrain.adjust_cliff_textures,
            ini.terrain.three_way_blends,
        )
    };
    assert_eq!(fields(&ini), (false, true));
    ini.read(b"GameData\n  MapName = NoName.map\nEnd\n")
        .unwrap();
    ini.read(b"GameData\n  AdjustCliffTextures = Yes\n  Use3WayTerrainBlends = 0\nEnd\n")
        .unwrap();
    assert_eq!(fields(&ini), (true, false));
    ini.read(b"GameData\n  adjustclifftextures = NO\n  Use3WayTerrainBlends = 2\nEnd\n")
        .unwrap();
    assert_eq!(fields(&ini), (false, true));
    // A box pays 100 by default, the shipped file's 75 once it reads; none of no money.
    assert_eq!(ini.box_value.get(), 100);
    ini.read(b"GameData\n  ValuePerSupplyBox = 75\nEnd\n")
        .unwrap();
    assert_eq!(ini.box_value.get(), 75);

    // The shipped camera's fields, which the camera's conversion reads.
    let mut shipped = GameDataIni::default();
    shipped
            .read(b"GameData\n CameraPitch = 37.5\n MaxCameraHeight = 310.0\n MinCameraHeight = 120.0\n HorizontalScrollSpeedFactor = 1.6;\n VerticalScrollSpeedFactor = 2.0\nEnd\n")
            .unwrap();
    assert_eq!(
        [
            shipped.camera_pitch,
            shipped.min_camera_height,
            shipped.max_camera_height,
            shipped.horizontal_scroll,
            shipped.vertical_scroll,
        ],
        [37.5, 120.0, 310.0, 1.6, 2.0]
    );

    let fails = |text: &[u8]| GameDataIni::default().read(text).unwrap_err();
    assert!(matches!(
        fails(b"GameData\n CameraPitch = steep\nEnd\n"),
        IniError::Real { line: 2, .. }
    ));
    // Three lights by default; a file may draw fewer, and none past the three a map keeps.
    assert_eq!(GameDataIni::default().global_lights(), 3);
    let mut two = GameDataIni::default();
    two.read(b"GameData\n NumberGlobalLights = 2\nEnd\n")
        .unwrap();
    assert_eq!(two.global_lights(), 2);
    assert_eq!(
        fails(b"GameData\n NumberGlobalLights = 4\nEnd\n"),
        IniError::Range { line: 2 }
    );
    assert_eq!(
        fails(b"GameData\n NumberGlobalLights = -1\nEnd\n"),
        IniError::Range { line: 2 }
    );
    for value in ["0", "-75"] {
        let text = format!("GameData\n ValuePerSupplyBox = {value}\nEnd\n");
        assert_eq!(fails(text.as_bytes()), IniError::Range { line: 2 });
    }
    assert_eq!(fails(b"MapName = A.map\n"), IniError::NotBlock { line: 1 });
    assert_eq!(fails(b"End\n"), IniError::StrayEnd { line: 1 });
    assert_eq!(
        fails(b"GameData\n AdjustCliffTextures = True\nEnd\n"),
        IniError::Bool { line: 2 }
    );
    assert!(matches!(
        fails(b"GameData\n Use3WayTerrainBlends = one\nEnd\n"),
        IniError::Number { line: 2, .. }
    ));
    assert_eq!(
        fails(b"GameData\n AdjustCliffTextures\nEnd\n"),
        IniError::NoValue { line: 2 }
    );
    assert_eq!(fails(b"GameData\n"), IniError::Unclosed);
}
