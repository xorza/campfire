use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops an object's contain module, or a part of one (transports.md, What the
/// slice leaves).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum ContainDropCause {
    /// An `OpenContain`.
    Open,
    /// A `CaveContain`, a map's cave that links to others.
    Cave,
    /// A `HealContain`, a hospital or a repair bay.
    Heal,
    /// An `InternetHackContain`, the Internet Center's hackers.
    InternetHack,
    /// A `RailedTransportContain`, a train.
    RailedTransport,
    /// A `RiderChangeContain`, the combat cycle's rider.
    RiderChange,
    /// A `MobNexusContain`, a mob's members.
    MobNexus,
    /// A `TunnelContain`, the GLA's tunnel network.
    Tunnel,
    /// An `OverlordContain`, the Overlord's and the Emperor's bunker.
    Overlord,
    /// A `HelixContain`, the Helix's bunker.
    Helix,
    /// A `ParachuteContain`, a parachute.
    Parachute,
    /// A container of no room, which takes no rider.
    NoRoom,
    /// A container that takes no unit that rides, by its kinds.
    NoRider,
    /// A container whose `AllowAlliesInside`, `AllowEnemiesInside` and `AllowNeutralInside` no
    /// relation of the engine's says, or which allows none.
    Relations,
    /// A container whose `AllowInsideKindOf` or `ForbidInsideKindOf` takes a rider otherwise than
    /// the tags of the mode can, or names a kind the import does not read.
    Kinds,
    /// A share past what a share holds, or an exit past the game's time.
    BadNumber,
    /// A payload of an object of no type that rides, or past the container's room: the container
    /// is made empty.
    Payload,
    /// A transport's `HealthRegen%PerSec`, as no rider heals inside.
    RiderHeal,
    /// A garrison that encloses no rider, which its riders fire from its station points, here
    /// from its place.
    StationPoints,
}
