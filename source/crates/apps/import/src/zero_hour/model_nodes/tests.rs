use super::*;
use crate::quaternion::Quaternion;
use crate::zero_hour::format::ini::object_ini::ini_recoil::IniRecoil;

/// A quarter turn about W3D's `y`, `−y` by the right hand, which raises `x` to `z`, up.
const RAISED: [f32; 4] = [
    0.0,
    -std::f32::consts::FRAC_1_SQRT_2,
    0.0,
    std::f32::consts::FRAC_1_SQRT_2,
];

/// A tank's pivots, each with its parent and its pose in its parent's frame: the root; the
/// turret, 3 m up; two numbered flashes, a barrel and a bare flash on the turret; on the root, a
/// bare flash and a slot's numbered fire bones beside a bare flash of its own; on the turret, a
/// rail, 2 m forward and 1 m up, turned to point up, whose number 02 the model lacks; and on the
/// root, a spark bone numbered 01 alone.
fn tank() -> ModelNodes {
    let at = |translation: [f32; 3]| RigidPose {
        translation: translation.into(),
        rotation: Quaternion::IDENTITY,
    };
    let rail = RigidPose {
        translation: [2.0, 0.0, 1.0].into(),
        rotation: Quaternion::from_array(RAISED),
    };
    let pivots = [
        ("roottransform", None, RigidPose::IDENTITY),
        ("turret", Some(0), at([0.0, 0.0, 3.0])),
        ("muzzlefx01", Some(1), RigidPose::IDENTITY),
        ("muzzlefx02", Some(1), RigidPose::IDENTITY),
        ("barrel", Some(1), RigidPose::IDENTITY),
        ("solo", Some(0), RigidPose::IDENTITY),
        ("flash", Some(0), RigidPose::IDENTITY),
        ("fire01", Some(0), RigidPose::IDENTITY),
        ("fire02", Some(0), RigidPose::IDENTITY),
        ("rail01", Some(1), rail),
        ("spark01", Some(0), RigidPose::IDENTITY),
    ]
    .map(|(name, parent, pose)| PivotName {
        name: name.to_owned(),
        parent,
        pose,
    });
    let sub_object = |name: &str, pivot, drawn, hidden| LookSubObject {
        name: name.to_owned(),
        pivot,
        drawn,
        hidden,
    };
    ModelNodes {
        sub_objects: vec![
            sub_object("tank.hull", 0, true, false),
            sub_object("tank.turret", 1, true, false),
            sub_object("tank.flash1", 2, true, false),
            sub_object("tank.flash2a", 3, true, false),
            sub_object("tank.flash2b", 3, true, false),
            sub_object("tank.barrel", 4, true, false),
            sub_object("tank.solo", 5, true, false),
            sub_object("tank.decal", 0, true, true),
            sub_object("tank.pickbox", 0, false, true),
            sub_object("tank.flashb", 6, true, false),
        ],
        pivots: pivots.to_vec(),
    }
}

fn bones(fire_fx: Option<&str>, muzzle_flash: &str) -> WeaponBones {
    WeaponBones {
        fire_fx: fire_fx.map(str::to_owned),
        muzzle_flash: Some(muzzle_flash.to_owned()),
        ..WeaponBones::default()
    }
}

#[test]
fn a_default_look_hides_flagged_parts_muzzle_flashes_and_its_hidden_sub_objects() {
    let nodes = tank();
    // Slot 1 numbers `MuzzleFX` 01 and 02 and stops at 03: the first sub-object on each bone.
    // Slot 2 finds `Fire01` and `Fire02` but no numbered flash, so not its bare `Flash` either.
    // Slot 3 numbers nothing, so its bare `Solo`. The decal is hidden by its flag; the box,
    // which has no node, is never named.
    let flashes = StateLook {
        hide_show: Vec::new(),
        weapons: [
            bones(None, "MuzzleFX"),
            bones(Some("Fire"), "Flash"),
            bones(None, "SOLO"),
        ],
    };
    assert_eq!(
        nodes.hidden(&flashes),
        ["tank.flash1", "tank.flash2a", "tank.solo", "tank.decal"]
    );
    // A show of the turret shows the sub-objects below it but its flashes, which hide after it,
    // the first on each of their bones: the second on `MuzzleFX02` shows.
    let shown = StateLook {
        hide_show: vec![HideShow {
            name: "turret".to_owned(),
            hide: false,
        }],
        weapons: [
            bones(None, "MuzzleFX"),
            WeaponBones::default(),
            WeaponBones::default(),
        ],
    };
    assert_eq!(
        nodes.hidden(&shown),
        ["tank.flash1", "tank.flash2a", "tank.decal"]
    );
    // Each change in order, by its full name or its name after the first `.`: the turret
    // hides the sub-objects on the pivots below its own, the barrel is shown again, and a name
    // of no sub-object is skipped.
    let change = |name: &str, hide| HideShow {
        name: name.to_owned(),
        hide,
    };
    let look = |hide_show| StateLook {
        hide_show,
        ..StateLook::default()
    };
    let turret = look(vec![
        change("TURRET", true),
        change("barrel", false),
        change("missing", true),
    ]);
    assert_eq!(
        nodes.hidden(&turret),
        [
            "tank.turret",
            "tank.flash1",
            "tank.flash2a",
            "tank.flash2b",
            "tank.decal"
        ]
    );
    // A sub-object on the root spreads to every sub-object off the root, as
    // `doHideShowBoneSubObjs` walks each sub-object's bone up to the root: the hull hides all,
    // then the decal shows all but the hull.
    let root = look(vec![change("tank.hull", true)]);
    assert_eq!(nodes.hidden(&root).len(), 9);
    let root = look(vec![change("tank.hull", true), change("Decal", false)]);
    assert_eq!(nodes.hidden(&root), ["tank.hull"]);
    assert_eq!(nodes.hidden(&StateLook::default()), ["tank.decal"]);
    // An upgrade's changes, each sub-object's last: the turret and those below it hidden, the
    // barrel shown again; a name of none in neither.
    let changes = [
        change("TURRET", true),
        change("barrel", false),
        change("missing", true),
    ];
    let bare = StateLook::default();
    assert_eq!(
        nodes.changes(&changes, &bare),
        NodeChanges {
            show: vec!["tank.barrel".to_owned()],
            hide: ["tank.turret", "tank.flash1", "tank.flash2a", "tank.flash2b"]
                .map(str::to_owned)
                .to_vec(),
        }
    );
    assert_eq!(nodes.changes(&[], &bare), NodeChanges::default());
    // An upgrade that shows the turret of a look of flashes leaves the flashes hidden.
    assert_eq!(
        nodes.changes(&[change("turret", false)], &shown),
        NodeChanges {
            show: ["tank.turret", "tank.flash2b", "tank.barrel"]
                .map(str::to_owned)
                .to_vec(),
            hide: Vec::new(),
        }
    );
    // The box, on the root, hides the 7 sub-objects off the root, but is never named itself, as
    // it is never drawn.
    let boxed = nodes.changes(&[change("pickbox", true)], &bare);
    assert_eq!(boxed.hide.len(), 7);
    assert!(!boxed.hide.contains(&"tank.pickbox".to_owned()));
}

#[test]
fn a_slot_fires_at_each_barrel_any_of_its_bones_numbers_or_at_its_bare_bone() {
    // `MuzzleFX` numbers two barrels, 01 and 02, of either case; `fire` its two, a barrel each,
    // as its flash `MuzzleFX` numbers them; `solo` its bare bone alone; `missing` none.
    let tank = tank();
    let recoil = BarrelRecoil::of(IniRecoil::default());
    let barrels = |fire: &str, flash: &str| {
        let barrels = tank.fire_barrels(&bones(Some(fire), flash), recoil);
        barrels
            .into_iter()
            .map(|barrel| barrel.bone)
            .collect::<Vec<_>>()
    };
    let named = |names: &[&str]| {
        names
            .iter()
            .map(|name| Some((*name).to_owned()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        barrels("MuzzleFX", "none"),
        named(&["muzzlefx01", "muzzlefx02"])
    );
    assert_eq!(barrels("fire", "none"), named(&["fire01", "fire02"]));
    assert_eq!(barrels("solo", "none"), named(&["solo"]));
    assert!(barrels("missing", "none").is_empty());
    // A flash numbered 01 and 02 makes two barrels: a fire bone numbered for neither leaves each
    // of no bone, each shown by its flash; one numbered for both gives each its own; one
    // numbered for the first alone gives the second the first's, as it has a flash.
    assert_eq!(barrels("solo", "MuzzleFX"), [None, None]);
    assert_eq!(barrels("fire", "MuzzleFX"), named(&["fire01", "fire02"]));
    assert_eq!(barrels("spark", "MuzzleFX"), named(&["spark01", "spark01"]));
    assert_eq!(barrels("spark", "none"), named(&["spark01"]));
    // Each flash is the first sub-object on its bone, shown for a frame, 33 ms, of a barrel of no
    // recoil bone, and for the 3 frames of the default recoil, 100 ms, of one of a recoil bone.
    let flash = |node: &str, ms| {
        Some(BarrelFlash {
            node: node.to_owned(),
            ms,
        })
    };
    let flashes = |bones: &WeaponBones| {
        let barrels = tank.fire_barrels(bones, recoil).into_iter();
        barrels.map(|barrel| barrel.flash).collect::<Vec<_>>()
    };
    assert_eq!(
        flashes(&bones(None, "MuzzleFX")),
        [flash("tank.flash1", 33), flash("tank.flash2a", 33)]
    );
    let recoiled = WeaponBones {
        recoil: Some("MuzzleFX".to_owned()),
        ..bones(None, "MuzzleFX")
    };
    assert_eq!(
        flashes(&recoiled),
        [flash("tank.flash1", 100), flash("tank.flash2a", 100)]
    );
    // A flash bone of no sub-object, the rail's, shows none.
    assert!(flashes(&bones(None, "rail")).is_empty());
}

#[test]
fn a_slots_barrels_launch_from_their_launch_bones_or_the_models_place() {
    // At rest, the rail stands on the turret 3 m up, at 2 m forward and 4 m up, turned up. Its
    // barrels, numbered as its fire bones number them: 01 at the rail, on glTF's axes
    // `(x, z, −y)`; 02, of no rail, at the model's place along its `x`. The rail alone numbers one
    // barrel; a launch bone the model lacks, none. The way is within 2⁻²² of up, as the turn's
    // `f32` steps round a half of √2 squared.
    let tank = tank();
    let rest = tank.pristine(None);
    let rail = |fire_fx: Option<&str>, pristine: &[RigidPose]| {
        let bones = WeaponBones {
            fire_fx: fire_fx.map(str::to_owned),
            launch: Some("Rail".to_owned()),
            ..WeaponBones::default()
        };
        tank.launches(&bones, pristine)
    };
    let barrels = rail(Some("fire"), &rest);
    assert_eq!(barrels.len(), 2);
    assert_eq!(barrels[0].at, Vec3::new(2.0, 4.0, 0.0));
    assert!(barrels[0].way.abs_diff_eq(Vec3::Y, 1.0 / 4_194_304.0));
    assert_eq!(
        barrels[1],
        ModelBarrel {
            at: Vec3::ZERO,
            way: Vec3::X,
        }
    );
    assert_eq!(rail(None, &rest).len(), 1);
    let missing = WeaponBones {
        fire_fx: Some("fire".to_owned()),
        launch: Some("missing".to_owned()),
        ..WeaponBones::default()
    };
    assert!(tank.launches(&missing, &rest).is_empty());
    // The turret's pivot, 3 m up; none for a name of no pivot.
    assert_eq!(
        tank.pivot_point("TURRET", &rest),
        Some(Vec3::new(0.0, 3.0, 0.0))
    );
    assert_eq!(tank.pivot_point("turretel", &rest), None);
    // An animation that lifts the turret to 5 m lifts the rail on it to 6 m; one of another
    // count of pivots places none, and the model stands at rest.
    let mut lifted: Vec<RigidPose> = tank.pivots.iter().map(|pivot| pivot.pose).collect();
    lifted[1].translation = Vec3::new(0.0, 0.0, 5.0);
    let animated = tank.pristine(Some(&lifted));
    assert_eq!(rail(None, &animated)[0].at, Vec3::new(2.0, 6.0, 0.0));
    assert_eq!(tank.pristine(Some(&lifted[1..])), rest);
}

#[test]
fn a_slots_rounds_are_its_hide_show_bone_or_its_numbered_launch_bones() {
    // Its `WeaponHideShowBone`, the turret: one round of the turret and what stands on its child
    // pivots, the flashes and the barrel.
    let tank = tank();
    let turret = WeaponBones {
        rounds: Some("Turret".to_owned()),
        launch: Some("missile".to_owned()),
        ..WeaponBones::default()
    };
    let names = |names: &[&str]| {
        names
            .iter()
            .map(|&name| name.to_owned())
            .collect::<Vec<_>>()
    };
    let bare = StateLook::default();
    assert_eq!(
        tank.rounds(&turret, &bare),
        [names(&[
            "tank.turret",
            "tank.flash1",
            "tank.flash2a",
            "tank.flash2b",
            "tank.barrel"
        ])]
    );
    // Of a look whose slot flashes at `MuzzleFX`, the round leaves out its flashes.
    let flashing = StateLook {
        weapons: [
            bones(None, "MuzzleFX"),
            WeaponBones::default(),
            WeaponBones::default(),
        ],
        ..StateLook::default()
    };
    assert_eq!(
        tank.rounds(&turret, &flashing),
        [names(&["tank.turret", "tank.flash2b", "tank.barrel"])]
    );
    // A rack's missiles 01, 02 and 04 under its launch bone `Missile`: a round each, 03 of none,
    // none past the last; a launch bone the model lacks, and a slot of neither, none.
    let sub_object = |name: &str, pivot| LookSubObject {
        name: name.to_owned(),
        pivot,
        drawn: true,
        hidden: false,
    };
    let rack = ModelNodes {
        sub_objects: vec![
            sub_object("rack.missile01", 0),
            sub_object("rack.missile02", 0),
            sub_object("rack.missile04", 0),
        ],
        pivots: Vec::new(),
    };
    let launch = |name: &str| WeaponBones {
        launch: Some(name.to_owned()),
        ..WeaponBones::default()
    };
    assert_eq!(
        rack.rounds(&launch("Missile"), &bare),
        [
            names(&["rack.missile01"]),
            names(&["rack.missile02"]),
            Vec::new(),
            names(&["rack.missile04"])
        ]
    );
    assert!(rack.rounds(&launch("rocket"), &bare).is_empty());
    assert!(rack.rounds(&WeaponBones::default(), &bare).is_empty());
}
