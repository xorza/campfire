use bevy::app::{App, Plugin, Startup};
use bevy::ecs::system::{Commands, Res};
use campfire_actions::ActionBook;
use campfire_values::{ActionId, NameId};

use crate::client_packages::ClientPackages;
use crate::name_texts::NameTexts;

/// The name players see of each action of the mode's packages, in its package's own language,
/// which the command panel shows: a train's or a build's, which the load requires; an action of
/// no name shows its id.
#[derive(Debug)]
pub(crate) struct ActionNamesPlugin;

impl Plugin for ActionNamesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::read);
    }
}

impl ActionNamesPlugin {
    /// Reads each action's name, as the action book numbers the actions.
    fn read(
        packages: Res<'_, ClientPackages>,
        book: Res<'_, ActionBook>,
        mut commands: Commands<'_, '_>,
    ) {
        let ClientPackages { packages, texts } = &*packages;
        let mut names: Vec<String> = book.names().iter().map(|(_, id)| id.to_owned()).collect();
        for view in packages.packages() {
            let header = &view.package.header;
            for (id, action) in view.content.actions() {
                let (Some(name), Some(at)) = (
                    &action.keys.name,
                    book.names().named((view.index, id.as_str())),
                ) else {
                    continue;
                };
                names[at.index()] = texts
                    .text(&header.name, name, &header.language)
                    .expect("the load checked each action's name");
            }
        }
        commands.insert_resource(names.into_iter().collect::<NameTexts<ActionId>>());
    }
}
