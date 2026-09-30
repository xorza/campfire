/// What kind of damage an amount is. The mode's `calc_damage` weighs each kind against its own
/// resistance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageKind {
    Physical,
    Magic,
    True,
}

impl DamageKind {
    pub const ALL: [DamageKind; 3] = [DamageKind::Physical, DamageKind::Magic, DamageKind::True];

    /// The kind named `name`.
    pub fn named(name: &str) -> Option<DamageKind> {
        DamageKind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The kind as scripts and data name it.
    pub const fn name(self) -> &'static str {
        match self {
            DamageKind::Physical => "physical",
            DamageKind::Magic => "magic",
            DamageKind::True => "true",
        }
    }
}
