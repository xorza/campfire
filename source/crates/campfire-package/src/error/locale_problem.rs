use std::fmt;

use fluent_syntax::parser::ParserError;

use crate::message_id::MessageId;

/// What is wrong with a file of human text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocaleProblem {
    /// Its name is not `<language>.ftl`, or in a locale package `<package>/<language>.ftl` of a
    /// package it depends on, the language an identifier in its canonical spelling.
    FileName,
    /// It does not parse as Fluent, first at this error.
    Parse(ParserError),
    /// It defines the message twice.
    Repeated(MessageId),
    /// A translation defines the message, which the file of its package's own language does not.
    Stray(MessageId),
}

impl fmt::Display for LocaleProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocaleProblem::FileName => {
                f.write_str("not <language>.ftl, its language in its canonical spelling")
            }
            LocaleProblem::Parse(error) => write!(f, "{error}"),
            LocaleProblem::Repeated(id) => write!(f, "message {id} twice"),
            LocaleProblem::Stray(id) => {
                write!(
                    f,
                    "message {id}, which the package's own language does not define"
                )
            }
        }
    }
}
