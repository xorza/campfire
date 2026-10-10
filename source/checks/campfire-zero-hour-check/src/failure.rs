use derive_more::Display;

/// What the imported Tournament Desert holds other than the original gives it.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// Its heights are not 270 × 340 samples.
    #[display("its heights are {columns} × {rows} samples, not 270 × 340")]
    Heights { columns: u32, rows: u32 },
    /// It places other than 580 units of 63 types.
    #[display("it places {units} units of {types} types, not 580 of 63")]
    Units { units: usize, types: usize },
    /// It has other than 28 markers.
    #[display("it has {_0} markers, not 28")]
    Markers(usize),
    /// Cells it draws have a tile with no texture.
    #[display("{_0} of its drawn cells have a tile with no texture")]
    Untextured(usize),
}
