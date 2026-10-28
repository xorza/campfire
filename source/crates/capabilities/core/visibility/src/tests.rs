use campfire_codec::Toml;
use serde::Deserialize;

use super::*;

/// A `data/mode.toml` that holds a `[visibility]` table, or none.
#[derive(Debug, Deserialize)]
struct ModeFile {
    #[serde(default)]
    visibility: ModeVisibility,
}

fn read(table: &str) -> Result<ModeVisibility, String> {
    Toml::parse::<ModeFile>(&format!("[visibility]\n{table}"))
        .map(|file| file.visibility)
        .map_err(|error| error.message().to_owned())
}

fn host(text: &str) -> SettingValue {
    text.parse().unwrap()
}

fn hosted(settings: &[SettingValue], max_spectators: u32) -> HostVisibility {
    HostVisibility {
        settings: settings.to_vec(),
        max_spectators: Some(max_spectators),
    }
}

const fn kind(kind: DataKind) -> Setting {
    Setting::Kind(kind)
}

#[test]
fn a_mode_sets_each_kind_within_what_it_may_and_the_rest_keeps_its_default() {
    let mode = read(
        r#"
inventory = ["allies", "enemies", "spectators"]
resources = ["spectators"]
life = ["spectators"]
unit_state_team = ["spectators"]
spectators = "teams"
out = "spectators"
host = ["spectators", "out", "inventory"]
"#,
    )
    .unwrap();
    let settings = mode.settings();
    let (allies, enemies, spectators) = (
        Recipients::ALLIES,
        Recipients::ENEMIES,
        Recipients::SPECTATORS,
    );
    // Each kind set is its list beside what the mode may not set: life keeps its allies, and
    // drops its enemies; a team field keeps its allies and adds spectators.
    for (data, audience) in [
        (DataKind::Inventory, Recipients::ALL),
        (DataKind::Resources, spectators),
        (DataKind::Life, allies | spectators),
        (DataKind::UnitStateTeam, allies | spectators),
        // Not set: the defaults.
        (DataKind::Pools, Recipients::NONE),
        (DataKind::Modifiers, Recipients::ALL),
        (DataKind::UnitStateOwner, Recipients::NONE),
        (DataKind::Unit, Recipients::ALL),
        (DataKind::Server, Recipients::NONE),
    ] {
        assert_eq!(settings.audience(data), audience, "{data:?}");
    }
    assert!(!settings.audience(DataKind::Life).contains(enemies));
    assert_eq!(settings.spectators(), SpectatorFog::Teams);
    assert_eq!(settings.out(), OutView::Spectators);
    assert_eq!(settings.max_spectators(), 0);
    assert!(mode.leaves(Setting::Spectators) && mode.leaves(kind(DataKind::Inventory)));
    assert!(!mode.leaves(kind(DataKind::Resources)));
    // It writes each setting a mode may set, and what it leaves the host, and reads back to
    // itself, as the default does.
    let written = |visibility: &ModeVisibility| {
        let text = Toml::write(&std::collections::BTreeMap::from([(
            "visibility",
            visibility,
        )]));
        Toml::parse::<ModeFile>(&text).unwrap().visibility
    };
    assert_eq!(written(&mode), mode);
    assert_eq!(
        written(&ModeVisibility::default()),
        ModeVisibility::default()
    );

    // No table: the defaults, which leave the spectators' fog and what a player out of play
    // sees to the host; an empty `host`, nothing.
    let none = Toml::parse::<ModeFile>("").unwrap().visibility;
    assert_eq!(none, ModeVisibility::default());
    assert_eq!(*none.settings(), Visibility::DEFAULT);
    assert!(none.leaves(Setting::Spectators) && none.leaves(Setting::Out));
    assert!(!none.leaves(kind(DataKind::Inventory)));
    let closed = read("host = []").unwrap();
    assert!(!closed.leaves(Setting::Spectators) && !closed.leaves(Setting::Out));
}

#[test]
fn each_flaw_of_a_modes_visibility_fails_its_read_with_its_own_error() {
    let unknown = |setting, value: &str| VisibilityError::UnknownValue {
        setting,
        value: value.to_owned(),
    };
    for (table, error) in [
        (
            r#"fog = "all""#,
            VisibilityError::UnknownSetting("fog".to_owned()),
        ),
        (
            r#"unit = ["allies"]"#,
            VisibilityError::NotSettable(DataKind::Unit),
        ),
        (
            "prediction = []",
            VisibilityError::NotSettable(DataKind::Prediction),
        ),
        (
            r#"life = ["allies"]"#,
            VisibilityError::Recipient {
                kind: DataKind::Life,
                recipient: Recipients::ALLIES,
            },
        ),
        (
            r#"unit_state_owner = ["enemies"]"#,
            VisibilityError::Recipient {
                kind: DataKind::UnitStateOwner,
                recipient: Recipients::ENEMIES,
            },
        ),
        (
            r#"inventory = ["owner"]"#,
            unknown(kind(DataKind::Inventory), "owner"),
        ),
        (
            r#"inventory = ["allies", "allies"]"#,
            VisibilityError::RepeatedRecipient {
                kind: DataKind::Inventory,
                recipient: Recipients::ALLIES,
            },
        ),
        (
            r#"spectators = "some""#,
            unknown(Setting::Spectators, "some"),
        ),
        (r#"out = "black""#, unknown(Setting::Out, "black")),
        (
            r#"host = ["out", "out"]"#,
            VisibilityError::Repeated(Setting::Out),
        ),
        (
            r#"host = ["unit"]"#,
            VisibilityError::NotSettable(DataKind::Unit),
        ),
        (
            r#"host = ["fog"]"#,
            VisibilityError::UnknownSetting("fog".to_owned()),
        ),
    ] {
        assert_eq!(read(table), Err(error.to_string()), "{table}");
    }
    // A value of another type than the setting takes.
    assert_eq!(
        read(r#"spectators = ["all"]"#),
        Err("invalid type: sequence, expected a string".to_owned())
    );
    assert_eq!(
        read(r#"inventory = "allies""#),
        Err("invalid type: string \"allies\", expected a sequence".to_owned())
    );
}

#[test]
fn a_host_sets_only_what_the_mode_leaves_it_each_once() {
    // As `--visibility` writes them: a kind's recipients by commas, none for the owner alone.
    let inventory = |recipients| SettingValue::Audience(DataKind::Inventory, recipients);
    assert_eq!(
        host("inventory=allies,enemies"),
        inventory(Recipients::ALLIES | Recipients::ENEMIES)
    );
    assert_eq!(host("inventory=").setting(), kind(DataKind::Inventory));
    assert_eq!(host("inventory="), inventory(Recipients::NONE));
    assert_eq!(
        host("spectators=teams"),
        SettingValue::Spectators(SpectatorFog::Teams)
    );
    assert_eq!(host("out=spectators").setting(), Setting::Out);
    assert_eq!(
        host("out=spectators"),
        SettingValue::Out(OutView::Spectators)
    );
    let unknown = |setting, value: &str| VisibilityError::UnknownValue {
        setting,
        value: value.to_owned(),
    };
    for (text, error) in [
        (
            "inventory",
            VisibilityError::NotHostSetting("inventory".to_owned()),
        ),
        (
            "inventory=allies,,enemies",
            unknown(kind(DataKind::Inventory), ""),
        ),
        (
            "spectators=all,teams",
            unknown(Setting::Spectators, "all,teams"),
        ),
        ("spectators=", unknown(Setting::Spectators, "")),
        ("unit=allies", VisibilityError::NotSettable(DataKind::Unit)),
    ] {
        assert_eq!(text.parse::<SettingValue>(), Err(error), "{text}");
    }

    // The mode leaves inventory and the spectators' fog; the host sets both, and admits two.
    let mode = read(r#"host = ["inventory", "spectators"]"#).unwrap();
    let session = mode
        .session(&hosted(
            &[host("inventory=allies"), host("spectators=teams")],
            2,
        ))
        .unwrap();
    assert_eq!(session.audience(DataKind::Inventory), Recipients::ALLIES);
    assert_eq!(session.spectators(), SpectatorFog::Teams);
    assert_eq!(session.max_spectators(), 2);
    assert_eq!(session.out(), OutView::Team);
    // Unset by the host: the mode's own, and no spectator unless the host says.
    assert_eq!(mode.session(&hosted(&[], 0)), Ok(*mode.settings()));
    for (given, error) in [
        (
            vec![host("resources=allies")],
            VisibilityError::NotLeft(kind(DataKind::Resources)),
        ),
        (
            vec![host("out=spectators")],
            VisibilityError::NotLeft(Setting::Out),
        ),
        (
            vec![host("inventory=allies"), host("inventory=enemies")],
            VisibilityError::Repeated(kind(DataKind::Inventory)),
        ),
    ] {
        assert_eq!(mode.session(&hosted(&given, 0)), Err(error), "{given:?}");
    }
    // A visibility the mode admits holds the mode's own setting of each it does not leave.
    assert!(mode.admits(&session) && mode.admits(mode.settings()));
    let mut kept = *mode.settings();
    kept.set(SettingValue::Audience(
        DataKind::Resources,
        Recipients::ALLIES,
    ))
    .unwrap();
    assert!(!mode.admits(&kept));
    let mut out = *mode.settings();
    out.set(SettingValue::Out(OutView::Spectators)).unwrap();
    assert!(!mode.admits(&out));
    assert!(mode.admits(&mode.settings().with_max_spectators(9)));
    let lives = read(r#"host = ["life"]"#).unwrap();
    assert_eq!(
        lives.session(&hosted(&[host("life=allies")], 0)),
        Err(VisibilityError::Recipient {
            kind: DataKind::Life,
            recipient: Recipients::ALLIES,
        })
    );
}

#[test]
fn a_visibility_encodes_as_its_bytes_and_reads_back_only_from_them() {
    // The kinds a mode may set, in their order: life, pools, modifiers, cooldowns, progression,
    // inventory, mode_state, resources, supply, queues, player_modifiers, the three unit state
    // fields' classes, the three player state fields', player_cooldowns and riders; then the fog,
    // what a player out of play sees, and the spectators.
    let default = [
        7, 0, 7, 0, 0, 0, 7, 0, 0, 0, 7, 0, 1, 7, 0, 1, 7, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    assert_eq!(Visibility::DEFAULT.encode(), default);
    assert_eq!(Visibility::decode(&default), Ok(Visibility::DEFAULT));

    let mode = read(
        r#"
inventory = ["allies", "spectators"]
life = ["spectators"]
host = ["spectators", "out"]
"#,
    )
    .unwrap();
    let session = mode
        .session(&hosted(
            &[host("spectators=teams"), host("out=spectators")],
            258,
        ))
        .unwrap();
    let bytes = session.encode();
    assert_eq!(
        bytes,
        [
            5, 0, 7, 0, 0, 5, 7, 0, 0, 0, 7, 0, 1, 7, 0, 1, 7, 0, 0, 1, 1, 2, 1, 0, 0
        ]
    );
    assert_eq!(Visibility::decode(&bytes), Ok(session));

    let flawed = |at: usize, byte: u8| {
        let mut bytes = default;
        bytes[at] = byte;
        Visibility::decode(&bytes)
    };
    // Life without its allies, which no mode takes; a unit field of the owner's to allies, and a
    // player's; a bit past the spectators'.
    assert_eq!(
        flawed(0, 6),
        Err(VisibilityDecodeError::Audience(DataKind::Life))
    );
    assert_eq!(
        flawed(11, 1),
        Err(VisibilityDecodeError::Audience(DataKind::UnitStateOwner))
    );
    assert_eq!(
        flawed(1, 8),
        Err(VisibilityDecodeError::Audience(DataKind::Pools))
    );
    assert_eq!(
        flawed(14, 1),
        Err(VisibilityDecodeError::Audience(DataKind::PlayerStateOwner))
    );
    assert_eq!(flawed(19, 2), Err(VisibilityDecodeError::Spectators));
    assert_eq!(flawed(20, 2), Err(VisibilityDecodeError::Out));
    assert_eq!(
        Visibility::decode(&default[..24]),
        Err(VisibilityDecodeError::Length(24))
    );
    assert_eq!(
        Visibility::decode(&[default.as_slice(), &[0]].concat()),
        Err(VisibilityDecodeError::Length(26))
    );
}
