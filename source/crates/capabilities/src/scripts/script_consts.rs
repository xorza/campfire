use campfire_script::rhai::ImmutableString;

use crate::units::action_id::ActionId;
use crate::units::track_id::TrackId;
use crate::units::unit_type::UnitType;
use crate::values::damage_kind::DamageKind;

/// The names scripts read, in the form they read them, built once from the books so that a read
/// allocates nothing: each action's, unit type's, track's and damage kind's, by id.
#[derive(Debug, Default)]
pub(crate) struct ScriptConsts {
    actions: Vec<ImmutableString>,
    unit_types: Vec<ImmutableString>,
    tracks: Vec<ImmutableString>,
    damage_kinds: Vec<ImmutableString>,
}

impl ScriptConsts {
    pub(crate) fn set_actions<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        fill(&mut self.actions, names);
    }

    pub(crate) fn set_unit_types<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        fill(&mut self.unit_types, names);
    }

    pub(crate) fn set_tracks<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        fill(&mut self.tracks, names);
    }

    pub(crate) fn set_damage_kinds<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        fill(&mut self.damage_kinds, names);
    }

    pub(crate) fn action(&self, id: ActionId) -> ImmutableString {
        self.actions[id.index()].clone()
    }

    pub(crate) fn unit_type(&self, unit_type: UnitType) -> ImmutableString {
        self.unit_types[unit_type.index()].clone()
    }

    pub(crate) fn track(&self, track: TrackId) -> ImmutableString {
        self.tracks[track.index()].clone()
    }

    pub(crate) fn damage_kind(&self, kind: DamageKind) -> ImmutableString {
        self.damage_kinds[kind.index()].clone()
    }

    /// The damage kind `name`, if the mode declares it.
    pub(crate) fn damage_kind_named(&self, name: &str) -> Option<DamageKind> {
        let at = self.damage_kinds.iter().position(|kind| kind == name)?;
        Some(DamageKind::new(
            u8::try_from(at).expect("the load keeps damage kinds within u8"),
        ))
    }
}

/// Puts `names`, in order, in place of what `held` holds.
fn fill<'a>(held: &mut Vec<ImmutableString>, names: impl Iterator<Item = &'a str>) {
    held.clear();
    held.extend(names.map(ImmutableString::from));
}
