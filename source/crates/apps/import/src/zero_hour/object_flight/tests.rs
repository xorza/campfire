use super::*;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::object_ini::samples::{DEFAULT_OBJECTS, OBJECTS};
use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
use crate::zero_hour::object_flight::samples::shell_projectile;

#[test]
fn a_shell_flies_its_weapons_speed_on_its_curve_and_hits_what_its_mask_names() {
    let steep = "Object Steep\n  Behavior = DumbProjectileBehavior M\n    \
                     FirstPercentIndent = 150%\n  End\nEnd\n";
    let ini = ObjectIni::of(&[DEFAULT_OBJECTS, OBJECTS, steep]);
    let locomotors = LocomotorIni::default();
    let fired_by = |object: &str, lines: &str| {
        let mut weapons = WeaponIni::default();
        let text = format!("Weapon W\n  WeaponSpeed = 400\n{lines}End\n");
        weapons.read(text.as_bytes()).unwrap();
        let object = ini.object(object.as_bytes()).unwrap();
        let flight = ObjectFlight::of(object, &locomotors).unwrap();
        flight.projectile(weapons.get("W").unwrap(), object, &RulesNames::default())
    };
    let fired = |lines: &str| fired_by("TankShell", lines);
    let collides =
        |mask: &str| fired(&format!("  ProjectileCollidesWith = {mask}\n")).map(|shell| shell.hits);
    assert_eq!(
        fired("  ProjectileCollidesWith = STRUCTURES WALLS\n"),
        Ok(shell_projectile(400))
    );
    // The default mask, structures; an ally's, an enemy's, and one's own structures; none.
    let hits = |filters: &[&str]| {
        let filters = filters.iter().map(|text| FilterData::parse(text).unwrap());
        Ok(ProjectileHits::Units(filters.collect()))
    };
    assert_eq!(
        fired("").map(|shell| shell.hits),
        hits(&["enemies:structure"])
    );
    assert_eq!(
        collides("ALLIES ENEMIES CONTROLLED_STRUCTURES"),
        hits(&["allies", "hostiles", "allies:structure"])
    );
    assert_eq!(collides("NONE"), Ok(ProjectileHits::Nothing));
    // Shrubbery, projectiles and missiles are no units; a speed of 0, and a point past the
    // way's end, are no numbers the engine takes.
    for mask in [
        "SHRUBBERY",
        "PROJECTILES",
        "SMALL_MISSILES",
        "BALLISTIC_MISSILES",
    ] {
        assert_eq!(collides(mask), Err(WeaponDropCause::Collides), "{mask}");
    }
    assert_eq!(
        fired("  WeaponSpeed = 0\n"),
        Err(WeaponDropCause::BadNumber)
    );
    assert_eq!(fired_by("Steep", ""), Err(WeaponDropCause::BadNumber));
}
