use std::error::Error;
use std::fmt;

/// An error with each error under it, from its own message down to its root cause, joined by
/// `: ` on one line, as `std::error::Report` writes one. An error's message holds only its own
/// step, and its source the rest, so a log writes an error through its report.
#[derive(Debug, Clone, Copy)]
pub struct ErrorReport<'a>(&'a (dyn Error + 'static));

impl<'a> ErrorReport<'a> {
    pub const fn of(error: &'a (dyn Error + 'static)) -> ErrorReport<'a> {
        ErrorReport(error)
    }
}

impl fmt::Display for ErrorReport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(cause) = source {
            write!(f, ": {cause}")?;
            source = cause.source();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
