use std::num::ParseFloatError;
use std::num::ParseIntError;

use thiserror::Error;

/// Why an INI file is none the game reads, each with its line from 1.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IniError {
    /// An `End` closes no block.
    #[error("line {line}: an End closes no block")]
    StrayEnd { line: usize },
    /// The file ends inside a block.
    #[error("it ends inside a block")]
    Unclosed,
    /// A line outside every block starts none of the file's blocks: `Object` or `ObjectReskin`
    /// in an object file, `Terrain` in a terrain file, `GameData` in a game data file.
    #[error("line {line}: no block of the file starts here")]
    NotBlock { line: usize },
    /// A block lacks its name, or an `ObjectReskin` its base's.
    #[error("line {line}: a block lacks a name")]
    NoName { line: usize },
    /// A field the importer reads has no value.
    #[error("line {line}: a field has no value")]
    NoValue { line: usize },
    /// A field of yes or no holds neither.
    #[error("line {line}: a field holds neither Yes nor No")]
    Bool { line: usize },
    /// A field of a real number holds none.
    #[error("line {line}: a field holds no number")]
    Real {
        line: usize,
        #[source]
        error: ParseFloatError,
    },
    /// A field's number is past what the game holds.
    #[error("line {line}: a field's number is past what the game holds")]
    Range { line: usize },
    /// A coordinate's field lacks a label, as `X:`, in its place, or the value after it.
    #[error("line {line}: a coordinate lacks a label or a value")]
    Coord { line: usize },
    /// A field of a whole number holds none.
    #[error("line {line}: a field holds no whole number")]
    Number {
        line: usize,
        #[source]
        error: ParseIntError,
    },
    /// A model draw module's first state is neither `DefaultConditionState` nor `NONE`.
    #[error("line {line}: a draw module's first state is no default")]
    NoDefaultState { line: usize },
    /// A `DefaultConditionState` follows another state.
    #[error("line {line}: a DefaultConditionState is not the first state")]
    LateDefaultState { line: usize },
    /// A model draw module's default state names no `Model`.
    #[error("line {line}: a default state names no model")]
    NoModel { line: usize },
    /// An `AliasConditionState` comes before any state, so it names none.
    #[error("line {line}: an alias state comes before any state")]
    FirstAlias { line: usize },
    /// A weapon bone names no slot of `PRIMARY`, `SECONDARY` and `TERTIARY`, or no bone.
    #[error("line {line}: a weapon bone names no slot and bone")]
    WeaponSlot { line: usize },
    /// A list of flags, as a `KindOf`, mixes names with names of `+` or `-`, or `NONE` with
    /// either.
    #[error("line {line}: a list of flags mixes its forms")]
    MixedFlags { line: usize },
    /// A value of a fixed list the game reads, as a surface or a geometry, is none of the list.
    #[error("line {line}: a value is none the game knows")]
    NotListed { line: usize },
    /// An object's `Scale` is not positive, or not finite.
    #[error("line {line}: an object's scale is not positive")]
    Scale { line: usize },
    /// An `Object` or an `ObjectReskin` of a name read before.
    #[error("line {line}: an object of this name is read before")]
    Redefined { line: usize },
    /// An `ObjectReskin` of a base not read before it.
    #[error("line {line}: a reskin's base is not read before it")]
    NoBase { line: usize },
}
