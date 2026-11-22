use bevy::asset::{Assets, Handle, uuid_handle};
use bevy::color::{Color, ColorToComponents, LinearRgba};
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::mesh::MeshTag;
use bevy::render::storage::ShaderBuffer;
use campfire_client_files::{TagMark, TeamColors};
use campfire_values::Team;

use crate::view::float_num::FloatNum;

/// The colors the models are painted by, in one buffer every file material reads: the match's
/// team colors, then each tint, in linear light: the build preview's, then each step of each tag
/// mark's fade, from its first to its whole tint. A mesh's tag names its team's place and its
/// tint's in it, so one material serves every team (02-engine-core.md, Team colors in the one
/// material; 02-engine-core.md, A tag marks its unit).
#[derive(Resource, Debug, Clone, PartialEq)]
pub(crate) struct MatchColors {
    teams: TeamColors,
    marks: Vec<[f32; 3]>,
}

/// How a model is painted: by its unit's team, none for a model of no unit's, and by its tint.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Paint {
    pub(crate) team: Option<Team>,
    pub(crate) tint: Option<Tint>,
}

/// A tint over a whole model, which adds to its light: red over a build that may not stand there,
/// as Zero Hour's `IllegalBuildColor` tints its placement's drawable; or the step `step`, from 0,
/// of the fade of the tag mark `mark`, by the mark's place among the HUD file's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tint {
    Blocked,
    Mark { mark: u8, step: u8 },
}

impl MatchColors {
    /// The buffer every file material reads, as the client starts.
    pub(crate) const BUFFER: Handle<ShaderBuffer> =
        uuid_handle!("6e3a3a5c-58f1-4a39-9d0e-0b8a2f6c7d41");

    /// The steps of a mark's fade, as Zero Hour's tint envelope steps its fade by its frames, 30
    /// in a second.
    pub(crate) const FADE_STEPS: u8 = 30;

    /// The colors of `teams` and of the tints of `marks`.
    pub(crate) fn new(teams: TeamColors, marks: &[TagMark]) -> Self {
        let marks = marks
            .iter()
            .map(|mark| mark.tint.map(FloatNum::float))
            .collect();
        Self { teams, marks }
    }

    /// Puts the colors in the buffer.
    pub(crate) fn upload(colors: Res<'_, Self>, mut buffers: ResMut<'_, Assets<ShaderBuffer>>) {
        buffers
            .insert(Self::BUFFER.id(), ShaderBuffer::from(colors.linear()))
            .expect("the buffer's handle is its own");
    }

    /// The place of `team` among the team colors.
    pub(crate) fn place(&self, team: Team) -> usize {
        self.teams.place(team)
    }

    /// The colors, in linear light, in the buffer's order.
    fn linear(&self) -> Vec<[f32; 4]> {
        let teams = self.teams.colors().iter().map(|&[r, g, b]| {
            let color = Color::srgb_u8(r, g, b).to_linear();
            color.to_f32_array()
        });
        let blocked = [LinearRgba::RED.to_f32_array()];
        let steps = f32::from(Self::FADE_STEPS);
        let marks = self.marks.iter().flat_map(|&tint| {
            (1..=Self::FADE_STEPS).map(move |step| {
                let [r, g, b] = tint.map(|channel| channel * f32::from(step) / steps);
                [r, g, b, 1.0]
            })
        });
        teams.chain(blocked).chain(marks).collect()
    }

    /// The tag of a mesh of `paint`: its team's place, from 1, in its low 16 bits, and its
    /// tint's, from 1, in its high 16, each 0 for none.
    pub(crate) fn tag(&self, paint: Paint) -> MeshTag {
        let team = paint.team.map_or(0, |team| self.teams.place(team) + 1);
        let tint = paint
            .tint
            .map_or(0, |tint| self.teams.colors().len() + tint.place() + 1);
        debug_assert!(
            paint.tint.is_none_or(
                |tint| tint.place() < 1 + self.marks.len() * usize::from(Self::FADE_STEPS)
            ),
            "a tint of a mark the match holds"
        );
        let tag = u32::try_from(team | (tint << 16)).expect("at most 256 colors and the tints");
        MeshTag::new(tag)
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
    /// Its place among the tints of the buffer.
    const fn place(self) -> usize {
        match self {
            Self::Blocked => 0,
            Self::Mark { mark, step } => {
                1 + mark as usize * MatchColors::FADE_STEPS as usize + step as usize
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::DeclaredName;
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_tag_names_its_team_s_place_low_and_its_tint_s_high_each_from_one() {
        // Two teams' colors, then the one tint: team 3 is the second's place, 2, and the tint
        // is third, so 3 in the high half.
        let teams = TeamColors::new(vec![[255, 0, 0], [0, 0, 255]]).unwrap();
        let colors = MatchColors::new(teams, &[]);
        let tag = |team: Option<u8>, tint| {
            colors
                .tag(Paint {
                    team: team.map(Team::new),
                    tint,
                })
                .value
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
        // A mark's tint of −0.5 follows the build's, in 30 steps of its fade: team 3 and the
        // mark's first step, the third color past the two teams', 4, and its last, 33.
        let mark = TagMark {
            tag: DeclaredName::new("disabled").unwrap(),
            tint: [-Num::HALF; 3],
            fade_ms: 1000,
            stills: true,
        };
        let colors = MatchColors::new(
            TeamColors::new(vec![[255, 0, 0], [0, 0, 255]]).unwrap(),
            &[mark],
        );
        let tag = |step| {
            let tint = Some(Tint::Mark { mark: 0, step });
            colors.tag(Paint { team: None, tint }).value
        };
        assert_eq!(tag(0), 4 << 16);
        assert_eq!(tag(29), 33 << 16);
        let linear = colors.linear();
        assert_eq!(linear.len(), 2 + 1 + 30);
        let first = -0.5 * 1.0 / 30.0;
        assert_eq!(linear[3], [first, first, first, 1.0]);
        assert_eq!(linear[32], [-0.5, -0.5, -0.5, 1.0]);
    }
}
