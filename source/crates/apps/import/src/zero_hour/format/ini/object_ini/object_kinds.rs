use campfire_common::NamedEnum;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{FlagSet, IniLine};

/// The kinds of an object's `KindOf` that the import reads, each by its bit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ObjectKinds(u64);

/// A kind of `KindOf` the import reads, by its name in lowercase: those that decide who draws an
/// object a map places (`GameLogic::startNewGame`), an optimized tree and a prop the client alone
/// draws, an object a build clears, which is fluff, the client's too, unless it is a fence, and a
/// wall a unit walks on top of, which the game spawns with the bridges; a structure, which routes
/// block when it is not small; an object that counts for a multiplayer victory, which a structure
/// of it keeps its player in the match by (09-zero-hour.md, Defeat by the last building that
/// counts); infantry, which a weapon may scatter more at; a vehicle, which drives through a crowd
/// as one that turns, not one that steps (04-capabilities/navigation.md, A unit moves by its kind
/// of motion); an aircraft, which a weapon may prefer (combat-response.md, An attack may prefer
/// its targets); an immobile object, a drone and one that cannot retaliate, which answer no
/// ally's call (combat-response.md, The import converts what the choice and the answer read); a
/// hulk; an object a capture puts on the radar; the kinds a container's masks name, and a
/// garrison that holds riders until it dies (transports.md, A container says who rides); and the
/// faction structures, `KINDOFMASK_FS`, the base defense among them, which a human player's idle
/// unit takes as a target among structures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
pub(crate) enum ObjectKind {
    OptimizedTree,
    Prop,
    ClearedByBuild,
    WalkOnTopOfWall,
    Structure,
    MpCountForVictory,
    Infantry,
    Vehicle,
    Aircraft,
    Immobile,
    Drone,
    CannotRetaliate,
    Hulk,
    Capturable,
    HugeVehicle,
    Dozer,
    Parachutable,
    Projectile,
    #[name = "forceattackable"]
    ForceAttackable,
    NoGarrison,
    GarrisonableUntilDestroyed,
    FsFactory,
    FsBaseDefense,
    FsTechnology,
    FsSupplyDropzone,
    FsSuperweapon,
    FsBlackMarket,
    FsSupplyCenter,
    FsStrategyCenter,
    FsFake,
    FsInternetCenter,
    FsAdvancedTech,
    FsBarracks,
    FsWarfactory,
    FsAirfield,
}

/// A mask of `KindOf` as a container's `AllowInsideKindOf` and `ForbidInsideKindOf` read it, and
/// whether it names a kind the import does not read, which no conversion can test.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct KindMask {
    pub(crate) kinds: ObjectKinds,
    pub(crate) unread: bool,
}

impl ObjectKinds {
    /// The faction structures, `KINDOFMASK_FS`.
    const FACTION: Self = Self::of(&[
        ObjectKind::FsFactory,
        ObjectKind::FsBaseDefense,
        ObjectKind::FsTechnology,
        ObjectKind::FsSupplyDropzone,
        ObjectKind::FsSuperweapon,
        ObjectKind::FsBlackMarket,
        ObjectKind::FsSupplyCenter,
        ObjectKind::FsStrategyCenter,
        ObjectKind::FsFake,
        ObjectKind::FsInternetCenter,
        ObjectKind::FsAdvancedTech,
        ObjectKind::FsBarracks,
        ObjectKind::FsWarfactory,
        ObjectKind::FsAirfield,
    ]);

    /// The kinds of `kinds`.
    pub(crate) const fn of(kinds: &[ObjectKind]) -> Self {
        let mut of = Self(0);
        let mut at = 0;
        while at < kinds.len() {
            of = of.with(kinds[at]);
            at += 1;
        }
        of
    }

    /// It with `kind`.
    pub(crate) const fn with(self, kind: ObjectKind) -> Self {
        Self(self.0 | Self::bit(kind))
    }

    /// Whether it holds `kind`.
    pub(crate) const fn holds(self, kind: ObjectKind) -> bool {
        self.0 & Self::bit(kind) != 0
    }

    /// Whether it holds any kind of `mask`, as `isAnyKindOf` tests it.
    pub(crate) const fn any_of(self, mask: Self) -> bool {
        self.0 & mask.0 != 0
    }

    /// The kinds it and `other` both hold.
    pub(crate) const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Whether it holds a faction structure's kind, as `Object::isFactionStructure` tests it.
    pub(crate) const fn faction(self) -> bool {
        self.any_of(Self::FACTION)
    }

    /// Sets the kinds by a `KindOf` line's flags, as `BitFlags::parse` does; a kind the import
    /// does not read changes nothing.
    pub(crate) fn set_by(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        at.set_flags(self)
    }

    const fn bit(kind: ObjectKind) -> u64 {
        1 << kind as u64
    }

    /// Sets or clears `kind`.
    const fn set_kind(&mut self, kind: ObjectKind, on: bool) {
        if on {
            self.0 |= Self::bit(kind);
        } else {
            self.0 &= !Self::bit(kind);
        }
    }
}

impl FlagSet for ObjectKinds {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn set(&mut self, name: &str, on: bool) -> bool {
        if let Some(kind) = ObjectKind::named(&name.to_ascii_lowercase()) {
            self.set_kind(kind, on);
        }
        true
    }
}

impl KindMask {
    /// The mask of `kinds` alone.
    pub(crate) const fn of(kinds: ObjectKinds) -> Self {
        Self {
            kinds,
            unread: false,
        }
    }

    /// Sets the mask by a line's flags, as `KindOfMaskType::parseFromINI` does.
    pub(crate) fn set_by(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        at.set_flags(self)
    }
}

impl FlagSet for KindMask {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn set(&mut self, name: &str, on: bool) -> bool {
        match ObjectKind::named(&name.to_ascii_lowercase()) {
            Some(kind) => self.kinds.set_kind(kind, on),
            None => self.unread |= on,
        }
        true
    }
}
