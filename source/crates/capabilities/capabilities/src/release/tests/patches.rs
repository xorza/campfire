//! Each patch a tool writes reads back through its section's reader, over the defaults or the
//! data it lies on (03-game-scripting.md, A section has a partial form).

use std::collections::BTreeSet;

use campfire_codec::TomlTable;
use campfire_combat::{
    ArmorShare, CombatData, CombatPatch, ModeCombatData, ModeCombatPatch, OnDeath, TaggedArmorData,
    TurretData, TurretPatch,
};
use campfire_declarations::SectionWriter;
use campfire_geometry::Bounds;
use campfire_math::Num;
use campfire_mode::{MapData, MapPatch, MarkerData, ModeData, ModePatch};
use campfire_units::BoundsData;
use campfire_values::TextId;

use super::*;

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

#[test]
fn each_patch_reads_back_through_its_sections_reader() {
    let readers = Release::sections();
    // A unit type of a `combat` of its armors alone and a `turret` of its turn rate alone, under
    // defaults of the rest: each section holds its patch's fields and its defaults'.
    let heavy: TaggedArmorData = Toml::parse("when = \"all:upgraded\"\narmor = \"heavy\"").unwrap();
    let mut unit = SectionWriter::default();
    unit.patch::<CombatData>(&CombatPatch {
        armor: Some(name("plate")),
        armors: Some(vec![heavy.clone()]),
        ..CombatPatch::default()
    })
    .patch::<TurretData>(&TurretPatch {
        turn_rate: Some(Num::int(180)),
        ..TurretPatch::default()
    });
    let unit = unit.into_table();
    assert_eq!(
        Toml::write(&unit),
        "[combat]\narmor = \"plate\"\n\n[[combat.armors]]\narmor = \"heavy\"\nwhen = \"all:upgraded\"\n\n[turret]\nturn_rate = \"180\"\n"
    );
    let units = TomlValue::of(&BTreeMap::from([(name("tank"), unit)]));
    let defaults: TomlTable = Toml::parse(
        "combat = { on_death = \"stay\" }\nturret = { tolerance_deg = 2, slot = \"weapon\" }",
    )
    .unwrap();
    let defaults = UnitDefaults::read(TomlValue::from(defaults), readers).unwrap();
    let types = Release::read_units(units.clone(), &defaults).unwrap();
    let tank = &types[&name("tank")];
    let combat = CombatData {
        on_death: OnDeath::Stay,
        armor: Some(name("plate")),
        armors: vec![heavy],
    };
    assert_eq!(tank.section::<CombatData>(), Some(&combat));
    let turret = TurretData::new(Num::int(180), Num::int(2), name("weapon"));
    assert_eq!(tank.section::<TurretData>(), turret.as_ref());
    // With no default under it, the turret lacks what its patch leaves out, and its read says
    // which; the combat takes its own defaults.
    let bare = Release::read_units(units, &UnitDefaults::default()).unwrap_err();
    assert!(
        bare.to_string().contains("missing field `tolerance_deg`"),
        "{bare}"
    );

    // The mode's own keys of its name and a timer, and the patch of its `[combat]` of one armor:
    // the rest of each as a mode that gives none reads it.
    let armor = ArmorShare::new(Num::int(2)).unwrap();
    let armors = BTreeMap::from([(name("plate"), BTreeMap::from([(name("fire"), armor)]))]);
    let mut mode = SectionWriter::default();
    mode.own::<ModeData>(&ModePatch {
        name: Some(TextId::new("mode-name").unwrap()),
        timers: Some(BTreeSet::from([name("wave")])),
        ..ModePatch::default()
    })
    .patch::<ModeCombatData>(&ModeCombatPatch {
        armors: Some(armors.clone()),
        ..ModeCombatPatch::default()
    });
    let data = TomlValue::from(mode.into_table())
        .read_with(ModeData::seed(readers))
        .unwrap();
    assert_eq!(
        (data.name.as_str(), data.timers.len(), data.params.len()),
        ("mode-name", 1, 0)
    );
    let combat = ModeCombatData {
        armors,
        ..ModeCombatData::default()
    };
    assert_eq!(data.section::<ModeCombatData>(), Some(&combat));

    // A map's bounds and its own keys of one marker, of no unit.
    let bounds = Bounds::new([Num::int(-4), Num::int(-4)], [Num::int(4), Num::int(4)]).unwrap();
    let marker: MarkerData =
        Toml::parse("name = \"spawn\"\ntags = [\"start\"]\nposition = [\"0\", \"0\"]").unwrap();
    let mut map = SectionWriter::default();
    map.section(&BoundsData(bounds)).own::<MapData>(&MapPatch {
        markers: Some(vec![marker.clone()]),
        ..MapPatch::default()
    });
    let map = TomlValue::from(map.into_table())
        .read_with(MapData::seed(readers))
        .unwrap();
    assert_eq!(
        (map.units.len(), map.markers.as_slice()),
        (0, [marker].as_slice())
    );
    assert_eq!(map.section::<BoundsData>(), Some(&BoundsData(bounds)));
}
