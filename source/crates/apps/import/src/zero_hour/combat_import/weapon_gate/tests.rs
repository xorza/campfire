use super::*;

#[test]
fn a_slot_whose_weapon_the_upgrade_set_keeps_serves_both_and_each_other_serves_its_set() {
    let set = |weapons: [Option<&str>; 3]| WeaponSet {
        weapons: weapons.map(|weapon| weapon.map(str::to_owned)),
        ..WeaponSet::default()
    };
    let starting = set([Some("Rifle"), Some("Pistol"), None]);
    // No upgrade set: each weapon serves both.
    assert_eq!(
        WeaponGate::of(&starting, None),
        [("Rifle", WeaponGate::Both), ("Pistol", WeaponGate::Both)]
    );
    // The upgrade keeps the rifle, drops the pistol, and adds a tertiary flamer.
    let upgrade = set([Some("Rifle"), None, Some("Flamer")]);
    assert_eq!(
        WeaponGate::of(&starting, Some(&upgrade)),
        [
            ("Rifle", WeaponGate::Both),
            ("Pistol", WeaponGate::Starting),
            ("Flamer", WeaponGate::Upgraded),
        ]
    );
    let id = DeclaredName::new("weapon_rifle").unwrap();
    let ids = [WeaponGate::Both, WeaponGate::Starting, WeaponGate::Upgraded]
        .map(|gate| gate.id(&id).unwrap().as_str().to_owned());
    assert_eq!(
        ids,
        [
            "weapon_rifle",
            "weapon_rifle_starting",
            "weapon_rifle_upgraded"
        ]
    );
    let long = DeclaredName::new(&"w".repeat(DeclaredName::MAX_LEN)).unwrap();
    assert_eq!(
        WeaponGate::Upgraded.id(&long),
        Err(WeaponDropCause::LongName)
    );
    let names = RulesNames::default();
    assert_eq!(WeaponGate::Both.when(&names), None);
    assert_eq!(
        WeaponGate::Upgraded.when(&names),
        Some(names.weapon_upgraded.clone())
    );
}
