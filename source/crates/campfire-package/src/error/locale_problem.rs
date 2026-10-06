use fluent_syntax::parser::ParserError;
use thiserror::Error;

use crate::message_id::MessageId;

/// What is wrong with a file of human text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LocaleProblem {
    /// Its name is not `<language>.ftl`, or in a locale package `<package>/<language>.ftl` of a
    /// package it depends on, the language an identifier in its canonical spelling.
    #[error("not <language>.ftl, its language in its canonical spelling")]
    FileName,
    /// It does not parse as Fluent, first at this error.
    #[error(transparent)]
    Parse(ParserError),
    /// It defines the message twice.
    #[error("message {0} twice")]
    Repeated(MessageId),
    /// A translation defines the message, which the file of its package's own language does not.
    #[error("message {0}, which the package's own language does not define")]
    Stray(MessageId),
}
