mod defaults;
mod flaws;
mod items;
mod moba;
mod mode_package;
mod module_rules;
mod rules;
mod texts;
mod uses;

use campfire_common::DeclaredName;
use campfire_package::Place;

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

/// The 3v3's fountain, its shop type.
fn fountain() -> Place {
    Place::UnitType(name("fountain"))
}
