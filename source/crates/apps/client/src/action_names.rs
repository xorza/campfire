use std::sync::Arc;

use bevy::app::{App, Plugin, Startup};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res};
use campfire_actions::ActionBook;
use campfire_package::{ModePackages, Texts};
use campfire_values::{ActionId, NameId};

/// The name players see of each action of the mode's packages, in its package's own language,
/// which the command panel shows: a train's or a build's, which the load requires; an action of
/// no name shows its id.
#[derive(Debug)]
pub(crate) struct ActionNames {
    pub(crate) packages: Arc<ModePackages>,
    /// The mode's text, which the client reads once for every panel.
    pub(crate) texts: Arc<Texts>,
}

/// Each action's name, by its id.
#[derive(Resource, Debug)]
pub(crate) struct ActionTexts(Vec<String>);

/// The packages whose action names the client shows.
#[derive(Resource, Debug)]
struct NamedPackages {
    packages: Arc<ModePackages>,
    texts: Arc<Texts>,
}

impl Plugin for ActionNames {
    fn build(&self, app: &mut App) {
        app.insert_resource(NamedPackages {
            packages: Arc::clone(&self.packages),
            texts: Arc::clone(&self.texts),
        });
        app.add_systems(Startup, Self::read);
    }
}

impl ActionNames {
    /// Reads each action's name, as the action book numbers the actions.
    fn read(
        packages: Res<'_, NamedPackages>,
        book: Res<'_, ActionBook>,
        mut commands: Commands<'_, '_>,
    ) {
        let NamedPackages { packages, texts } = &*packages;
        let mut names: Vec<String> = book.names().iter().map(|(_, id)| id.to_owned()).collect();
        for view in packages.packages() {
            let header = &view.package.header;
            for (id, action) in view.content.actions() {
                let (Some(name), Some(at)) =
                    (&action.name, book.names().named((view.index, id.as_str())))
                else {
                    continue;
                };
                names[at.index()] = texts
                    .text(&header.name, name, &header.language)
                    .expect("the load checked each action's name");
            }
        }
        commands.insert_resource(ActionTexts(names));
    }
}

impl ActionTexts {
    /// The name of `action`.
    pub(crate) fn of(&self, action: ActionId) -> &str {
        &self.0[action.index()]
    }
}
