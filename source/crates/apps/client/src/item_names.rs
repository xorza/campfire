use std::sync::Arc;

use bevy::app::{App, Plugin, Startup};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res};
use campfire_items::{ItemBook, ItemId};
use campfire_package::{ModePackages, Texts};
use campfire_values::NameId;

/// The name players see of each item type of the mode, in the mode's own language, which the
/// shop panel and the inventory bar show.
#[derive(Debug)]
pub(crate) struct ItemNames {
    pub(crate) packages: Arc<ModePackages>,
}

/// Each item type's name, by its id.
#[derive(Resource, Debug)]
pub(crate) struct ItemTexts(Vec<String>);

/// The packages whose item names the client shows.
#[derive(Resource, Debug)]
struct NamedPackages(Arc<ModePackages>);

impl Plugin for ItemNames {
    fn build(&self, app: &mut App) {
        app.insert_resource(NamedPackages(Arc::clone(&self.packages)));
        app.add_systems(Startup, ItemNames::read);
    }
}

impl ItemNames {
    /// Reads each item's name, as the item book numbers the items: the load checked that the
    /// mode's own language gives each.
    fn read(
        packages: Res<'_, NamedPackages>,
        book: Res<'_, ItemBook>,
        mut commands: Commands<'_, '_>,
    ) {
        let packages = &packages.0;
        let texts = Texts::new(packages, &[]).expect("a mode's own text, with no translation");
        let mode = packages.mode().package;
        let header = &mode.header;
        let mut names = vec![String::new(); book.names().len()];
        for (id, item) in &packages.content().items {
            let at = book
                .names()
                .named(id.as_str())
                .expect("an item of the book");
            names[at.index()] = texts
                .text(header.name.as_str(), &item.name, &header.language)
                .expect("the load checked each item's name");
        }
        commands.insert_resource(ItemTexts(names));
    }
}

impl ItemTexts {
    /// The name of `item`.
    pub(crate) fn of(&self, item: ItemId) -> &str {
        &self.0[item.index()]
    }
}
