use std::sync::Arc;

use campfire_actions::{EffectData, EffectTo, LaunchLists, ListedEffect};
use campfire_areas::LandEffectData;
use campfire_codec::{Toml, TomlTable};
use campfire_combat::HealEffectData;
use campfire_progression::XpEffectData;
use campfire_values::{Number, Scalar};

use super::*;
use crate::release::Release;

#[test]
fn a_package_reads_its_unit_types_and_its_sections_and_no_other_key() {
    let readers = Release::sections();
    let read = |text: &str| {
        let raw = Toml::parse::<TomlTable>(text).unwrap();
        raw.read_with(PackageContent::seed(&readers, &UnitDefaults::default()))
    };
    let content = read("[units.scout]\ntags = [\"fast\"]\n[actions]\n[modifiers]").unwrap();
    let units: Vec<&str> = content.units.keys().map(DeclaredName::as_str).collect();
    assert_eq!(units, ["scout"]);
    assert_eq!(
        content.section::<ActionsData>(),
        Some(&ActionsData::default())
    );
    assert_eq!(
        PackageContent::keys(&readers).collect::<Vec<_>>(),
        ["units", "actions", "modifiers", "items"]
    );
    assert_eq!(
        read("[unit]").unwrap_err().message(),
        "unknown field `unit`, expected one of `units`, `actions`, `modifiers`, `items`"
    );
    // An action's lists read each effect as the module of its key registers it, a launch's
    // own lists among them, and `to` beside it.
    let bolt = |list: &str| {
        let text = format!("[actions.bolt]\ntargeting = \"none\"\non_resolve = {list}");
        read(&text).map(|content| {
            let actions = content.section::<ActionsData>().unwrap();
            actions.0[&DeclaredName::new("bolt").unwrap()]
                .on_resolve
                .clone()
        })
    };
    let name = |name: &str| DeclaredName::new(name).unwrap();
    let one = || Number::Value(Scalar::Int(1));
    let heal: Arc<dyn ListedEffect> = Arc::new(HealEffectData { amount: one() });
    let xp: Arc<dyn ListedEffect> = Arc::new(XpEffectData {
        track: name("valor"),
        amount: one(),
    });
    let launch: Arc<dyn ListedEffect> = Arc::new(LandEffectData {
        area: name("blast"),
        lists: LaunchLists {
            on_hit: Vec::new(),
            on_end: vec![EffectData {
                does: xp,
                to: EffectTo::Reached,
            }],
        },
    });
    let list = r#"[{ heal = { amount = 1 }, to = "source" }, { land = { area = "blast", on_end = [{ xp = { track = "valor", amount = 1 } }] } }]"#;
    assert_eq!(
        bolt(list).unwrap(),
        [
            EffectData {
                does: heal,
                to: EffectTo::Source,
            },
            EffectData {
                does: launch,
                to: EffectTo::Reached,
            },
        ]
    );
    // Exactly one effect's key, of a module or planned, at every depth.
    let unknown = "unknown field `bogus`, expected one of `spawn`, `modifier`, `purge`, `restore`, `xp`, `damage`, `heal`, `move`, `land`, `loot`, `noise`, `to`";
    for (list, message) in [
        ("[{}]", "exactly one effect"),
        (r#"[{ to = "source" }]"#, "exactly one effect"),
        (
            "[{ heal = { amount = 1 }, loot = {} }]",
            "exactly one effect",
        ),
        ("[{ bogus = {} }]", unknown),
        (
            r#"[{ land = { area = "blast", on_hit = [{ bogus = {} }] } }]"#,
            unknown,
        ),
    ] {
        let error = bolt(list).unwrap_err();
        assert_eq!(error.message(), message, "{list}");
    }
    assert!(bolt("[{ loot = { anything = 1 } }]").is_ok());
}
