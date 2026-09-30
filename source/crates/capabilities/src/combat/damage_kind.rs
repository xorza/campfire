/// What kind of damage an amount is. The mode's `calc_damage` weighs each kind against its own
/// resistance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageKind {
    Physical,
    Magic,
    True,
}

impl DamageKind {
    /// A kind as scripts and data name it: `physical`, `magic` or `true`.
    pub fn parse(text: &str) -> Option<DamageKind> {
        match text {
            "physical" => Some(DamageKind::Physical),
            "magic" => Some(DamageKind::Magic),
            "true" => Some(DamageKind::True),
            _ => None,
        }
    }
}
