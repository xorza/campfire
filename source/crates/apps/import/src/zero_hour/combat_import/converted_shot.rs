use campfire_common::DeclaredName;

/// An action the import converts a weapon to, and how it shoots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConvertedShot<'a> {
    pub(crate) action: &'a DeclaredName,
    pub(crate) shot: WeaponShot<'a>,
}

/// How the import converts a weapon's action: as a shot of no projectile, or as one that fires
/// projectiles of the unit type it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WeaponShot<'a> {
    Shot,
    Projectile(&'a DeclaredName),
}
