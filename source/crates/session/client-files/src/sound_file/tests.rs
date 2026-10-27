use campfire_codec::Toml;

use super::*;

#[test]
fn a_sound_file_gives_its_volumes_listener_zoom_and_playlists_and_refuses_what_does_not_hold() {
    // Zero Hour's: positional sounds at 80 %, flat at 72 %, music at 55 %; the listener 50 m above
    // the ground or a third of the way to the camera; a near camera raising positional sounds by
    // 20 % from 425 m to 130; America's playlist of a track and the silence after it.
    let text = r#"
        volumes = { positional = 0.8, flat = 0.72, music = 0.55 }
        listener = { height = 50.0, share = 0.333 }
        zoom = { near = 130.0, far = 425.0, boost = 0.2 }

        [music]
        choice = "faction"
        playlists = { america = ["client/music/usa_01.mp3", "client/music/silence60.mp3"] }
    "#;
    let file = Toml::parse::<SoundFile>(text).unwrap();
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let path = |text: &str| PackagePath::parse(text).unwrap();
    assert_eq!(
        file,
        SoundFile {
            volumes: SoundVolumes {
                positional: 0.8,
                flat: 0.72,
                music: 0.55,
            },
            listener: Some(Listener {
                height: 50.0,
                share: 0.333,
            }),
            zoom: Some(ZoomBoost {
                near: 130.0,
                far: 425.0,
                boost: 0.2,
            }),
            music: Some(Music {
                choice: name("faction"),
                playlists: BTreeMap::from([(
                    name("america"),
                    vec![
                        path("client/music/usa_01.mp3"),
                        path("client/music/silence60.mp3"),
                    ],
                )]),
            }),
        }
    );
    assert_eq!(SoundFile::decode(&file.encode()).unwrap(), file);
    // An empty file plays each class whole, at the camera, with no boost and no music.
    let empty = Toml::parse::<SoundFile>("").unwrap();
    assert_eq!(empty, SoundFile::default());
    assert_eq!(empty.volumes.positional, 1.0);

    // A volume past 1, a listener's share past 1 or a height below 0, a zoom whose far is not
    // past its near, and a playlist of no track are refused.
    let refused = |from: &str, to: &str| {
        let text = text.replacen(from, to, 1);
        Toml::parse::<SoundFile>(&text).unwrap_err().to_string()
    };
    assert!(refused("music = 0.55", "music = 1.2").contains("volume"));
    assert!(refused("share = 0.333", "share = 1.5").contains("listener"));
    assert!(refused("height = 50.0", "height = -1.0").contains("listener"));
    assert!(refused("far = 425.0", "far = 130.0").contains("zoom"));
    let silent = refused(
        "[\"client/music/usa_01.mp3\", \"client/music/silence60.mp3\"]",
        "[]",
    );
    assert!(silent.contains("`america`"), "{silent}");
}
