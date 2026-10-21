use std::sync::Arc;

use bevy::DefaultPlugins;
use bevy::app::{App, PluginGroup};
use bevy::asset::AssetApp;
use bevy::asset::io::AssetSourceBuilder;
use bevy::log::LogPlugin;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use bevy::window::{Window, WindowPlugin};
use campfire_app_threads::OneThreadPlugins;
use campfire_bots::BotScript;
use campfire_common::EnumAll;
use campfire_declarations::CapabilitySet;
use campfire_net::{Pace, PacedRunnerPlugin};
use campfire_package::ModePackages;
use campfire_sim::Capability;

use crate::action_names::ActionNamesPlugin;
use crate::bot::BotPlugin;
use crate::build_preview::BuildPreviewPlugin;
use crate::command_panel::CommandPanelPlugin;
use crate::hud::HudPlugin;
use crate::inventory_bar::InventoryBarPlugin;
use crate::item_names::ItemNamesPlugin;
use crate::local_keys::LocalKeysPlugin;
use crate::orders::OrdersPlugin;
use crate::selecting::SelectingPlugin;
use crate::shop_panel::ShopPanelPlugin;
use crate::view::ViewPlugin;
use crate::view::area_view::AreaViewPlugin;
use crate::view::client_data::ClientFilesPlugin;
use crate::view::package_source::PackageAssets;
use crate::view::player_camera::PlayerCameraPlugin;
use crate::view::terrain_view::TerrainViewPlugin;
use crate::view::unit_models::UnitModelsPlugin;

/// A part of a window's end, one entry of the client's one list: the matches it joins, the
/// capabilities a mode holds for it, and the plugin it adds, so a client of any mode adds what the
/// mode's capabilities serve and no system takes a part of a capability the mode lacks
/// (02-engine-core.md, A client module list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub(crate) enum ClientModule {
    /// The window, its renderer, and the packages' asset source.
    Window,
    /// The keys of a local match: its pause, its speed, its saves.
    LocalKeys,
    ClientFiles,
    View,
    AreaView,
    PlayerCamera,
    UnitModels,
    TerrainView,
    Hud,
    Selecting,
    Orders,
    ItemNames,
    ShopPanel,
    InventoryBar,
    ActionNames,
    CommandPanel,
    BuildPreview,
}

/// The client's own end: a bot's, which plays its script with no window, or a window's, which
/// draws the match, of a match on a local server whose pace its keys change, or of a remote one.
#[derive(Debug)]
pub(crate) enum ClientEnd {
    Bot(BotScript),
    Window { local: Option<Arc<Pace>> },
}

/// What the modules of a window's end read as they add their plugins: the mode's packages, and
/// the pace of a local match.
#[derive(Debug)]
struct WindowCx<'a> {
    packages: &'a Arc<ModePackages>,
    local: Option<&'a Arc<Pace>>,
}

impl ClientModule {
    /// The capabilities a mode holds for the module to join its client: production's books for
    /// the panel and the action names, the items' for the shop and the inventory, a build's box,
    /// which only navigation tests, for the build preview, and the areas' reach for their discs.
    const fn needs(self) -> &'static [Capability] {
        match self {
            Self::AreaView => &[Capability::Areas],
            Self::ItemNames | Self::ShopPanel | Self::InventoryBar => &[Capability::Items],
            Self::ActionNames | Self::CommandPanel => &[Capability::Production],
            Self::BuildPreview => &[Capability::Production, Capability::Navigation],
            Self::Window
            | Self::LocalKeys
            | Self::ClientFiles
            | Self::View
            | Self::PlayerCamera
            | Self::UnitModels
            | Self::TerrainView
            | Self::Hud
            | Self::Selecting
            | Self::Orders => &[],
        }
    }

    /// Whether the module joins a window's end of a match on a local server, `local`, or a
    /// remote one: the local keys a local match's alone, every other module either.
    const fn joins(self, local: bool) -> bool {
        match self {
            Self::LocalKeys => local,
            Self::Window
            | Self::ClientFiles
            | Self::View
            | Self::AreaView
            | Self::PlayerCamera
            | Self::UnitModels
            | Self::TerrainView
            | Self::Hud
            | Self::Selecting
            | Self::Orders
            | Self::ItemNames
            | Self::ShopPanel
            | Self::InventoryBar
            | Self::ActionNames
            | Self::CommandPanel
            | Self::BuildPreview => true,
        }
    }

    /// The modules of the list that join a window's end, `local` or not, in a mode of
    /// `capabilities`, in the list's order.
    pub(crate) fn of(local: bool, capabilities: CapabilitySet) -> impl Iterator<Item = Self> {
        Self::ALL.into_iter().filter(move |module| {
            module.joins(local)
                && module
                    .needs()
                    .iter()
                    .all(|&capability| capabilities.contains(capability))
        })
    }
}

impl ClientEnd {
    /// Adds the end to `app`: a bot's, with no window, or each module of the list that joins a
    /// window's end in the mode of `packages`, in the list's order, as Bevy builds its asset
    /// sources as its asset plugin is added, so the window's source goes first.
    pub(crate) fn add(self, app: &mut App, packages: &Arc<ModePackages>) {
        let local = match self {
            Self::Bot(script) => {
                app.add_plugins((
                    OneThreadPlugins,
                    TimePlugin,
                    StatesPlugin,
                    PacedRunnerPlugin,
                    BotPlugin { script },
                ));
                return;
            }
            Self::Window { local } => local,
        };
        let cx = WindowCx {
            packages,
            local: local.as_ref(),
        };
        let capabilities = packages.manifest().capabilities;
        for module in ClientModule::of(local.is_some(), capabilities) {
            module.add(app, &cx);
        }
    }
}

impl ClientModule {
    /// Adds the module's plugin to the window's end `app`, of `cx`.
    fn add(self, app: &mut App, cx: &WindowCx<'_>) {
        let packages = cx.packages;
        match self {
            Self::Window => {
                // Every reader the builder makes shares the one source's bound on the models
                // that load.
                let source = PackageAssets::of(packages);
                app.register_asset_source(
                    PackageAssets::NAME,
                    AssetSourceBuilder::new(move || Box::new(source.clone())),
                );
                let window = WindowPlugin {
                    primary_window: Some(Window {
                        title: "Campfire".to_owned(),
                        ..Window::default()
                    }),
                    ..WindowPlugin::default()
                };
                app.add_plugins(DefaultPlugins.set(window).disable::<LogPlugin>());
            }
            Self::LocalKeys => {
                let pace = cx.local.expect("the local keys join a local match alone");
                app.add_plugins(LocalKeysPlugin {
                    pace: Arc::clone(pace),
                });
            }
            Self::ClientFiles => {
                app.add_plugins(ClientFilesPlugin {
                    packages: Arc::clone(packages),
                });
            }
            Self::View => {
                app.add_plugins(ViewPlugin);
            }
            Self::AreaView => {
                app.add_plugins(AreaViewPlugin);
            }
            Self::PlayerCamera => {
                app.add_plugins(PlayerCameraPlugin);
            }
            Self::UnitModels => {
                app.add_plugins(UnitModelsPlugin);
            }
            Self::TerrainView => {
                app.add_plugins(TerrainViewPlugin);
            }
            Self::Hud => {
                app.add_plugins(HudPlugin);
            }
            Self::Selecting => {
                app.add_plugins(SelectingPlugin {
                    control: packages.data().players.control,
                });
            }
            Self::Orders => {
                app.add_plugins(OrdersPlugin);
            }
            Self::ItemNames => {
                app.add_plugins(ItemNamesPlugin);
            }
            Self::ShopPanel => {
                app.add_plugins(ShopPanelPlugin);
            }
            Self::InventoryBar => {
                app.add_plugins(InventoryBarPlugin);
            }
            Self::ActionNames => {
                app.add_plugins(ActionNamesPlugin);
            }
            Self::CommandPanel => {
                app.add_plugins(CommandPanelPlugin);
            }
            Self::BuildPreview => {
                app.add_plugins(BuildPreviewPlugin);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::schedule::Schedules;
    use campfire_test_packages::TestMode;

    use super::*;
    use crate::test_app;

    #[test]
    fn every_module_s_systems_take_no_conflicting_access() {
        // Bevy checks that a system's queries do not conflict as it initializes the system: each
        // schedule of a window's end with every module, its window's headless, initialized.
        let mut app = test_app::headless();
        test_app::add_match(&mut app);
        let packages = Arc::new(TestMode::Lane.packages());
        let pace = Arc::new(Pace::default());
        let cx = WindowCx {
            packages: &packages,
            local: Some(&pace),
        };
        for module in ClientModule::ALL {
            if module != ClientModule::Window {
                module.add(&mut app, &cx);
            }
        }
        app.finish();
        app.cleanup();
        let world = app.world_mut();
        let labels: Vec<_> = (world.resource::<Schedules>().iter())
            .map(|(_, schedule)| schedule.label())
            .collect();
        for label in labels {
            world.schedule_scope(label, |world, schedule| {
                schedule.initialize(world).unwrap();
            });
        }
    }

    #[test]
    fn a_window_adds_the_modules_whose_capabilities_its_mode_holds() {
        let (remote, local) = (false, true);
        let of = |local, capabilities: &[Capability]| {
            let set = (capabilities.iter()).fold(CapabilitySet::CORE, |set, &of| set.with(of));
            ClientModule::of(local, set).collect::<Vec<_>>()
        };
        // A remote window of no capability: the core's modules, in the list's order.
        let core = [
            ClientModule::Window,
            ClientModule::ClientFiles,
            ClientModule::View,
            ClientModule::PlayerCamera,
            ClientModule::UnitModels,
            ClientModule::TerrainView,
            ClientModule::Hud,
            ClientModule::Selecting,
            ClientModule::Orders,
        ];
        assert_eq!(of(remote, &[]), core);
        // A local one adds its keys; production alone adds the panel and its names, and the
        // preview only with navigation; items the shop, the inventory and their names; areas their
        // discs.
        let with = |extra: &[ClientModule], local_keys: bool| {
            let mut modules = core.to_vec();
            if local_keys {
                modules.insert(1, ClientModule::LocalKeys);
            }
            modules.extend_from_slice(extra);
            modules.sort_by_key(|module| ClientModule::ALL.iter().position(|of| of == module));
            modules
        };
        assert_eq!(of(local, &[]), with(&[], true));
        let panel = [ClientModule::ActionNames, ClientModule::CommandPanel];
        assert_eq!(of(remote, &[Capability::Production]), with(&panel, false));
        let built = [Capability::Production, Capability::Navigation];
        let preview = [&panel[..], &[ClientModule::BuildPreview]].concat();
        assert_eq!(of(remote, &built), with(&preview, false));
        let shop = [
            ClientModule::ItemNames,
            ClientModule::ShopPanel,
            ClientModule::InventoryBar,
        ];
        assert_eq!(of(remote, &[Capability::Items]), with(&shop, false));
        let areas = [ClientModule::AreaView];
        assert_eq!(of(remote, &[Capability::Areas]), with(&areas, false));
    }
}
