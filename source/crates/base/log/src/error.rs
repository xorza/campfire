use std::env::VarError;
use std::error::Error;
use std::fmt;

use tracing_subscriber::filter::ParseError;

/// Why an environment variable that is set holds no log filter.
#[derive(Debug)]
pub(crate) enum FilterRefused {
    /// Its value is not Unicode.
    NotUnicode(VarError),
    /// Its value is no filter.
    NotFilter(ParseError),
}

impl fmt::Display for FilterRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FilterRefused::NotUnicode(_) => "the value is not Unicode",
            FilterRefused::NotFilter(_) => "the value is no filter",
        })
    }
}

impl Error for FilterRefused {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FilterRefused::NotUnicode(error) => Some(error),
            FilterRefused::NotFilter(error) => Some(error),
        }
    }
}
