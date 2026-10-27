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
    // No particle lives by default, the shipped 2,500 once it reads.
    assert_eq!(ini.max_particles, 0);
    ini.read(b"GameData\n  MaxParticleCount = 2500\nEnd\n")
        .unwrap();
    assert_eq!(ini.max_particles, 2500);
    // Gravity: −1 a frame squared by default, −900 a second squared at 30 frames a second; the
    // shipped −64 once it reads.
    assert_eq!(ini.gravity, -900.0);
    ini.read(b"GameData\n  Gravity = -64.0\nEnd\n").unwrap();
    assert_eq!(ini.gravity, -64.0);
    // No track by default, of none at once; the shipped 100 once they read, and a count past a
    // `u16` refused.
    assert_eq!((ini.track_marks, ini.most_tracks), (false, 0));
    ini.read(b"GameData\n  MakeTrackMarks = Yes\n  MaxTerrainTracks = 100\nEnd\n")
        .unwrap();
    assert_eq!((ini.track_marks, ini.most_tracks), (true, 100));
    assert!(
        ini.read(b"GameData\n  MaxTerrainTracks = 70000\nEnd\n")
            .is_err()
    );
    // The ground's stiffness: 0.5 by default, the shipped 0.8 once it reads.
    assert_eq!(ini.ground_stiffness, 0.5);
    ini.read(b"GameData\n  GroundStiffness = 0.8\nEnd\n")
        .unwrap();
    assert_eq!(ini.ground_stiffness, 0.8);
    // The damage thresholds, 0.5 and 0.1 by default, the shipped 0.7 and 0.35 once they read.
    let thresholds = |ini: &GameDataIni| [ini.damaged_threshold, ini.really_damaged_threshold];
    assert_eq!(thresholds(&ini), [0.5, 0.1]);
    ini.read(
        b"GameData
  UnitDamagedThreshold = 0.7
  UnitReallyDamagedThreshold = 0.35
End
",
    )
    .unwrap();
    assert_eq!(thresholds(&ini), [0.7, 0.35]);
    // A sale gives back all of its cost by default, the shipped 50 % once it reads.
    assert_eq!(ini.sell_share, 1.0);
    ini.read(b"GameData\n  SellPercentage = 50%\nEnd\n")
        .unwrap();
    assert_eq!(ini.sell_share, 0.5);
    // The shroud: white, with clear cells whole, fogged ones at 127 and shrouded ones dark, until
    // a file sets them; a level past a byte refused.
    assert_eq!(
        (ini.shroud_color, ini.shroud_levels),
        ([255; 3], [255, 127, 0])
    );
    ini.read(
        b"GameData\n  ShroudColor = R:0 G:64 B:128\n  FogAlpha = 100\n  ShroudAlpha = 20\nEnd\n",
    )
    .unwrap();
    assert_eq!(
        (ini.shroud_color, ini.shroud_levels),
        ([0, 64, 128], [255, 100, 20])
    );
    assert!(ini.read(b"GameData\n  ClearAlpha = 256\nEnd\n").is_err());
    assert!(
        ini.read(b"GameData\n  ShroudColor = R:0 G:300 B:0\nEnd\n")
            .is_err()
    );

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
    assert_eq!(
        fails(b"GameData\n MaxParticleCount = -1\nEnd\n"),
        IniError::Range { line: 2 }
    );
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
