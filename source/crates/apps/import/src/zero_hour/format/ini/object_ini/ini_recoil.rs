use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// A draw module's recoil of a barrel at its shot, as its lines write it: its speed at the shot
/// a second, `InitialRecoilSpeed`, none for `W3DModelDrawModuleData`'s of a frame; the most it
/// moves, `MaxRecoilDistance`; and the share of its speed each frame keeps, `RecoilDamping`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IniRecoil {
    pub(crate) speed: Option<f32>,
    pub(crate) most: f32,
    pub(crate) damping: f32,
}

impl IniRecoil {
    /// Reads a line of a draw module of the recoil's fields: whether it is one.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        match word {
            "initialrecoilspeed" => self.speed = Some(at.real()?),
            "maxrecoildistance" => self.most = at.real()?,
            "recoildamping" => self.damping = at.real()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}

/// `W3DModelDrawModuleData`'s, 3 and 0.4.
impl Default for IniRecoil {
    fn default() -> Self {
        Self {
            speed: None,
            most: 3.0,
            damping: 0.4,
        }
    }
}
