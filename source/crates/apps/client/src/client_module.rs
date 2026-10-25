use std::path::{Path, PathBuf};
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
use campfire_common::EnumAll;
use campfire_declarations::CapabilitySet;
use campfire_host::{ClientLayout, Pace, ReportCamera};
use campfire_net::PacedRunnerPlugin;
use campfire_package::ModePackages;
use campfire_sim::Capability;

use crate::action_names::ActionNamesPlugin;
use crate::build_preview::BuildPreviewPlugin;
use crate::command_panel::CommandPanelPlugin;
use crate::gather_click::GatherClickPlugin;
use crate::hud::HudPlugin;
use crate::inventory_bar::InventoryBarPlugin;
use crate::item_names::ItemNamesPlugin;
use crate::learn_keys::LearnKeysPlugin;
use crate::local_keys::LocalKeysPlugin;
use crate::orders::OrdersPlugin;
use crate::pointer::unit_picking::UnitPickingPlugin;
use crate::rally_click::RallyClickPlugin;
use crate::report::ReportPlugin;
use crate::selecting::SelectingPlugin;
use crate::shop_panel::ShopPanelPlugin;
use crate::use_click::UseClickPlugin;
use crate::view::ViewPlugin;
use crate::view::area_view::AreaViewPlugin;
use crate::view::client_data::ClientFilesPlugin;
use crate::view::look_states::LookStatesPlugin;
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
    /// The report chord, of a window that keeps a data directory.
    Report,
    ClientFiles,
    View,
    AreaView,
    PlayerCamera,
    UnitPicking,
    UnitModels,
    LookStates,
    TerrainView,
    Hud,
    Selecting,
    Orders,
    GatherClick,
    UseClick,
    RallyClick,
    LearnKeys,
    ItemNames,
    ShopPanel,
    InventoryBar,
    ActionNames,
    CommandPanel,
    BuildPreview,
}

/// The client's own end: a bot's, whose script the sim client plays, with no window, or a
/// window's, which draws the match, of a match on a local server whose pace its keys change, or
/// of a remote one, with the client's data directory when it keeps one, the directory of the
/// mode it plays, and a replayed report's camera, where the window's starts.
#[derive(Debug)]
pub(crate) enum ClientEnd {
    Bot,
    Window {
        local: Option<Arc<Pace>>,
        data: Option<ClientLayout>,
        mode_dir: PathBuf,
        camera: Option<ReportCamera>,
    },
}

/// What the modules of a window's end read as they add their plugins: the mode's packages and
/// directory, the pace of a local match, the client's data directory, and a replayed report's
/// camera.
#[derive(Debug)]
struct WindowCx<'a> {
    packages: &'a Arc<ModePackages>,
    mode_dir: &'a Path,
    local: Option<&'a Arc<Pace>>,
    data: Option<&'a ClientLayout>,
    camera: Option<&'a ReportCamera>,
}

/// What a window's end keeps of its match, which some modules need: no data directory, the
/// client's data directory of a remote match, or that of a local match, whose server runs in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowKeeps {
    Nothing,
    Remote,
    Local,
}

impl ClientModule {
    /// The capabilities a mode holds for the module to join its client: production's books for
    /// the panel and the action names, the items' for the shop and the inventory, a build's box,
    /// which only navigation tests, for the build preview, and the areas' reach for their discs.
    const fn needs(self) -> &'static [Capability] {
        match self {
            Self::AreaView => &[Capability::Areas],
            Self::ItemNames | Self::ShopPanel | Self::InventoryBar => &[Capability::Items],
            Self::ActionNames | Self::CommandPanel | Self::GatherClick | Self::RallyClick => {
                &[Capability::Production]
            }
            Self::UseClick => &[Capability::Interaction],
            Self::LearnKeys => &[Capability::Progression],
            Self::BuildPreview => &[Capability::Production, Capability::Navigation],
            Self::Window
            | Self::LocalKeys
            | Self::Report
            | Self::ClientFiles
            | Self::View
            | Self::PlayerCamera
            | Self::UnitPicking
            | Self::UnitModels
            | Self::LookStates
            | Self::TerrainView
            | Self::Hud
            | Self::Selecting
            | Self::Orders => &[],
        }
    }

    /// Whether the module joins a window's end that keeps `keeps`: the local keys a local
    /// match's alone, the report chord's one that keeps a data directory, every other module any.
    const fn joins(self, keeps: WindowKeeps) -> bool {
        match self {
            Self::LocalKeys => matches!(keeps, WindowKeeps::Local),
            Self::Report => !matches!(keeps, WindowKeeps::Nothing),
            Self::Window
            | Self::ClientFiles
            | Self::View
            | Self::AreaView
            | Self::PlayerCamera
            | Self::UnitPicking
            | Self::UnitModels
            | Self::LookStates
            | Self::TerrainView
            | Self::Hud
            | Self::Selecting
            | Self::Orders
            | Self::GatherClick
            | Self::UseClick
            | Self::RallyClick
            | Self::LearnKeys
            | Self::ItemNames
            | Self::ShopPanel
            | Self::InventoryBar
            | Self::ActionNames
            | Self::CommandPanel
            | Self::BuildPreview => true,
        }
    }

    /// The modules of the list that join a window's end that keeps `keeps`, in a mode of
    /// `capabilities`, in the list's order.
    pub(crate) fn of(
        keeps: WindowKeeps,
        capabilities: CapabilitySet,
    ) -> impl Iterator<Item = Self> {
        Self::ALL.into_iter().filter(move |module| {
            module.joins(keeps)
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
        let (local, data, mode_dir, camera) = match self {
            Self::Bot => {
                app.add_plugins((
                    OneThreadPlugins,
                    TimePlugin,
                    StatesPlugin,
                    PacedRunnerPlugin,
                ));
                return;
            }
            Self::Window {
                local,
                data,
                mode_dir,
                camera,
            } => (local, data, mode_dir, camera),
        };
        let keeps = match (&local, &data) {
            (Some(_), _) => WindowKeeps::Local,
            (None, Some(_)) => WindowKeeps::Remote,
            (None, None) => WindowKeeps::Nothing,
        };
        let cx = WindowCx {
            packages,
            mode_dir: &mode_dir,
            local: local.as_ref(),
            data: data.as_ref(),
            camera: camera.as_ref(),
        };
        let capabilities = packages.manifest().capabilities;
        for module in ClientModule::of(keeps, capabilities) {
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
            Self::Report => {
                let data = cx
                    .data
                    .expect("the report chord joins a window that keeps data alone");
                app.add_plugins(ReportPlugin {
                    data: data.clone(),
                    mode: packages.manifest().header.name.clone(),
                    mode_dir: cx.mode_dir.to_owned(),
                    map: packages.map_name().clone(),
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
                app.add_plugins(PlayerCameraPlugin {
                    start: cx.camera.copied(),
                });
            }
            Self::UnitPicking => {
                app.add_plugins(UnitPickingPlugin);
            }
            Self::UnitModels => {
                app.add_plugins(UnitModelsPlugin);
            }
            Self::LookStates => {
                app.add_plugins(LookStatesPlugin);
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
            Self::GatherClick => {
                app.add_plugins(GatherClickPlugin);
            }
            Self::UseClick => {
                app.add_plugins(UseClickPlugin);
            }
            Self::RallyClick => {
                app.add_plugins(RallyClickPlugin);
            }
            Self::LearnKeys => {
                app.add_plugins(LearnKeysPlugin);
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
        let data = ClientLayout::at(PathBuf::from("client"));
        let cx = WindowCx {
            packages: &packages,
            mode_dir: &TestMode::Lane.dir(),
            local: Some(&pace),
            data: Some(&data),
            camera: None,
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
        let (remote, local) = (WindowKeeps::Nothing, WindowKeeps::Local);
        let of = |keeps, capabilities: &[Capability]| {
            let set = (capabilities.iter()).fold(CapabilitySet::CORE, |set, &of| set.with(of));
            ClientModule::of(keeps, set).collect::<Vec<_>>()
        };
        // A remote window of no capability: the core's modules, in the list's order.
        let core = [
            ClientModule::Window,
            ClientModule::ClientFiles,
            ClientModule::View,
            ClientModule::PlayerCamera,
            ClientModule::UnitPicking,
            ClientModule::UnitModels,
            ClientModule::LookStates,
            ClientModule::TerrainView,
            ClientModule::Hud,
            ClientModule::Selecting,
            ClientModule::Orders,
        ];
        assert_eq!(of(remote, &[]), core);
        // A local one adds its keys and the report chord, and a remote one that keeps a data
        // directory the chord alone; production alone adds the gather and rally clicks, the
        // panel and its names, and the preview only with navigation; items the shop, the
        // inventory and their names; areas their discs; interaction the use click; progression
        // the learn keys.
        let with = |extra: &[ClientModule], keeps: WindowKeeps| {
            let mut modules = core.to_vec();
            if keeps == WindowKeeps::Local {
                modules.push(ClientModule::LocalKeys);
            }
            if keeps != WindowKeeps::Nothing {
                modules.push(ClientModule::Report);
            }
            modules.extend_from_slice(extra);
            modules.sort_by_key(|module| ClientModule::ALL.iter().position(|of| of == module));
            modules
        };
        assert_eq!(of(local, &[]), with(&[], local));
        let kept = WindowKeeps::Remote;
        assert_eq!(of(kept, &[]), with(&[], kept));
        let panel = [
            ClientModule::GatherClick,
            ClientModule::RallyClick,
            ClientModule::ActionNames,
            ClientModule::CommandPanel,
        ];
        assert_eq!(of(remote, &[Capability::Production]), with(&panel, remote));
        let built = [Capability::Production, Capability::Navigation];
        let preview = [&panel[..], &[ClientModule::BuildPreview]].concat();
        assert_eq!(of(remote, &built), with(&preview, remote));
        let shop = [
            ClientModule::ItemNames,
            ClientModule::ShopPanel,
            ClientModule::InventoryBar,
        ];
        assert_eq!(of(remote, &[Capability::Items]), with(&shop, remote));
        let areas = [ClientModule::AreaView];
        assert_eq!(of(remote, &[Capability::Areas]), with(&areas, remote));
        let interaction = [ClientModule::UseClick];
        assert_eq!(
            of(remote, &[Capability::Interaction]),
            with(&interaction, remote)
        );
        let progression = [ClientModule::LearnKeys];
        assert_eq!(
            of(remote, &[Capability::Progression]),
            with(&progression, remote)
        );
    }
}
