use bevy::asset::{Assets, Handle, uuid_handle};
use bevy::color::{Color, ColorToComponents, LinearRgba};
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res, ResMut};
use bevy::mesh::MeshTag;
use bevy::render::storage::ShaderBuffer;
use campfire_client_files::TeamColors;
use campfire_values::Team;

use crate::view::client_data::ClientData;

/// The colors the models are painted by, in one buffer every file material reads: the match's
/// team colors, then each tint, in linear light. A mesh's tag names its team's place and its
/// tint's in it, so one material serves every team (02-engine-core.md, Team colors in the one
/// material).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct MatchColors {
    teams: TeamColors,
}

/// How a model is painted: by its unit's team, none for a model of no unit's, and by its tint.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Paint {
    pub(crate) team: Option<Team>,
    pub(crate) tint: Option<Tint>,
}

/// A tint over a whole model, which adds to its light: red over a build that may not stand there,
/// as Zero Hour's `IllegalBuildColor` tints its placement's drawable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tint {
    Blocked,
}

impl MatchColors {
    /// The buffer every file material reads, as the client starts.
    pub(crate) const BUFFER: Handle<ShaderBuffer> =
        uuid_handle!("6e3a3a5c-58f1-4a39-9d0e-0b8a2f6c7d41");

    pub(crate) const fn new(teams: TeamColors) -> Self {
        Self { teams }
    }

    /// Puts the colors of the mode's team colors file in the buffer, and keeps them.
    pub(crate) fn upload(
        data: Res<'_, ClientData>,
        mut buffers: ResMut<'_, Assets<ShaderBuffer>>,
        mut commands: Commands<'_, '_>,
    ) {
        let colors = Self::new(data.team_colors.clone());
        buffers
            .insert(Self::BUFFER.id(), ShaderBuffer::from(colors.linear()))
            .expect("the buffer's handle is its own");
        commands.insert_resource(colors);
    }

    /// The colors, in linear light, in the buffer's order.
    fn linear(&self) -> Vec<[f32; 4]> {
        let teams = self.teams.colors().iter().map(|&[r, g, b]| {
            let color = Color::srgb_u8(r, g, b).to_linear();
            color.to_f32_array()
        });
        teams
            .chain(Tint::ALL.map(|tint| tint.color().to_f32_array()))
            .collect()
    }

    /// The tag of a mesh of `paint`: its team's place, from 1, in its low 16 bits, and its
    /// tint's, from 1, in its high 16, each 0 for none.
    pub(crate) fn tag(&self, paint: Paint) -> MeshTag {
        let team = paint.team.map_or(0, |team| self.teams.place(team) + 1);
        let tint = paint
            .tint
            .map_or(0, |tint| self.teams.colors().len() + tint.place() + 1);
        let tag = u32::try_from(team | (tint << 16)).expect("at most 256 colors and the tints");
        MeshTag(tag)
    }

    /// The colors of every team, in their places' order.
    pub(crate) fn teams(&self) -> impl Iterator<Item = Color> {
        self.teams
            .colors()
            .iter()
            .map(|&[r, g, b]| Color::srgb_u8(r, g, b))
    }
}

impl Paint {
    /// The paint of a model of a unit of `team`, of no tint.
    pub(crate) const fn of(team: Team) -> Self {
        Self {
            team: Some(team),
            tint: None,
        }
    }
}

impl Tint {
    const ALL: [Self; 1] = [Self::Blocked];

    const fn color(self) -> LinearRgba {
        match self {
            Self::Blocked => LinearRgba::RED,
        }
    }

    const fn place(self) -> usize {
        match self {
            Self::Blocked => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_names_its_team_s_place_low_and_its_tint_s_high_each_from_one() {
        // Two teams' colors, then the one tint: team 3 is the second's place, 2, and the tint
        // is third, so 3 in the high half.
        let teams = TeamColors::new(vec![[255, 0, 0], [0, 0, 255]]).unwrap();
        let colors = MatchColors::new(teams);
        let tag = |team: Option<u8>, tint| {
            colors
                .tag(Paint {
                    team: team.map(Team::new),
                    tint,
                })
                .0
        };
        assert_eq!(tag(None, None), 0);
        assert_eq!(tag(Some(0), None), 1);
        assert_eq!(tag(Some(3), None), 2);
        assert_eq!(tag(Some(3), Some(Tint::Blocked)), 2 | (3 << 16));
        assert_eq!(tag(None, Some(Tint::Blocked)), 3 << 16);
        // The buffer's colors, in linear light: sRGB's full red and blue are full, and the tint
        // red.
        assert_eq!(
            colors.linear(),
            [
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                [1.0, 0.0, 0.0, 1.0]
            ]
        );
        assert_eq!(colors.teams().count(), 2);
    }
}
