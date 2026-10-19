use bevy::app::{App, Plugin, Startup};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res};
use campfire_items::ItemBook;
use campfire_values::{ItemId, NameId};

use crate::client_packages::ClientPackages;

/// The name players see of each item type of the mode, in the mode's own language, which the
/// shop panel and the inventory bar show.
#[derive(Debug)]
pub(crate) struct ItemNames;

/// Each item type's name, by its id.
#[derive(Resource, Debug)]
pub(crate) struct ItemTexts(Vec<String>);

impl Plugin for ItemNames {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::read);
    }
}

impl ItemNames {
    /// Reads each item's name, as the item book numbers the items: the load checked that the
    /// mode's own language gives each.
    fn read(
        packages: Res<'_, ClientPackages>,
        book: Res<'_, ItemBook>,
        mut commands: Commands<'_, '_>,
    ) {
        let ClientPackages { packages, texts } = &*packages;
        let mode = packages.mode().package;
        let header = &mode.header;
        let mut names = vec![String::new(); book.names().len()];
        for (id, item) in packages.content().items() {
            let at = book
                .names()
                .named(id.as_str())
                .expect("an item of the book");
            names[at.index()] = texts
                .text(&header.name, &item.name, &header.language)
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
