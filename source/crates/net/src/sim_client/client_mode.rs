use campfire_capabilities::{Bounds, CapabilitySet, Grid, Metric, PoolId, Walker};
use campfire_package::ModePackages;
use campfire_runner::SessionRules;

/// The session a client can play: the rules of the mode it holds, with the capabilities the
/// mode declares, its map's metric, in which its predicted units measure their reach, its map's
/// bounds, which its predicted units stay within, its map's pathing grid and kinds of walker, on
/// which it plans their routes as the server does, and its life pool, which its predicted
/// targets need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMode {
    pub rules: SessionRules,
    pub capabilities: CapabilitySet,
    pub metric: Metric,
    pub bounds: Bounds,
    pub pathing: Option<Grid>,
    pub walkers: Vec<Walker>,
    pub life: Option<PoolId>,
}

impl ClientMode {
    pub fn of(packages: &ModePackages) -> ClientMode {
        ClientMode {
            rules: SessionRules::of(packages),
            capabilities: packages.manifest().capabilities,
            metric: packages.map().metric,
            bounds: packages.map().bounds,
            pathing: packages
                .map()
                .pathing()
                .expect("a loaded map's cells make a grid"),
            walkers: packages.walkers(),
            life: packages.data().combat.life_pool(&packages.data().pools),
        }
    }
}
