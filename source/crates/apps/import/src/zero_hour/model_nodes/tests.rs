use super::*;
use crate::zero_hour::format::ini::object_ini::HideShow;

/// A tank's pivots, each with its parent: the root; the turret; two numbered flashes, a
/// barrel and a bare flash on the turret; and on the root, a bare flash and a slot's
/// numbered fire bones beside a bare flash of its own.
fn tank() -> ModelNodes {
    let pivots = [
        ("roottransform", None),
        ("turret", Some(0)),
        ("muzzlefx01", Some(1)),
        ("muzzlefx02", Some(1)),
        ("barrel", Some(1)),
        ("solo", Some(0)),
        ("flash", Some(0)),
        ("fire01", Some(0)),
        ("fire02", Some(0)),
    ]
    .map(|(name, parent)| PivotName {
        name: name.to_owned(),
        parent,
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
}
