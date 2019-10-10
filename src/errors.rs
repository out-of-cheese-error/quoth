use thiserror::Error;

//
//#[derive(Error, Debug)]
//pub enum DataStoreError {
//    #[error("data store disconnected")]
//    Disconnect(#[source] io::Error),
//    #[error("the data for key `{0}` is not available")]
//    Redaction(String),
//    #[error("invalid header (expected {expected:?}, found {found:?})")]
//    InvalidHeader {
//        expected: String,
//        found: String,
//    },
//    #[error("unknown data store error")]
//    Unknown,
//}

/// Errors which can be caused by normal quoth operation.
/// Those caused by external libraries throw their own errors
#[derive(Debug, Error)]
pub enum QuothError {
    /// Thrown when trying to access an unrecorded author
    #[error("I don't know who {author:?} is.")]
    AuthorNotFound { author: String },
    /// Thrown when trying to access a nonexistent quote index
    #[error("You haven't written that quote: {index:?}.")]
    QuoteNotFound { index: usize },
    /// Thrown when trying to access an unrecorded book
    #[error("I haven't read {book:?} yet.")]
    BookNotFound { book: String },
    /// Thrown when trying to access an unrecorded tag
    #[error("You haven't tagged anything as {tag:?} yet.")]
    TagNotFound { tag: String },
    /// Thrown when no text is returned from an external editor
    #[error("Your editor of choice didn't work.")]
    EditorError,
    /// Thrown when argument is given empty input like '' or ' '
    #[error("Type something already!")]
    NoInputError,
    /// Catch-all for stuff that should never happen
    #[error("{message:?}\nRedo from start.")]
    OutOfCheeseError { message: String },
    /// Thrown when explicit Y not received from user for destructive things
    #[error("{message:?}\nDoing nothing.")]
    DoingNothing { message: String },
    /// Thrown when $HOME is not set
    #[error("$HOME not set")]
    Homeless,
    /// Thrown when badly formatted tsv file given for parsing
    #[error("I can't read {filename:?}. Make sure it has 'Quote', 'Book', and 'Author' columns and is tab-separated.")]
    FileParseError { filename: String },
}
