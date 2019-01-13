/// Errors which can be caused by normal quoth operation.
/// Those caused by external libraries throw their own errors
#[derive(Debug, Fail)]
pub enum QuothError {
    /// Thrown when trying to access an unrecorded author
    #[fail(display = "I don't know who {} is.", author)]
    AuthorNotFound { author: String },
    /// Thrown when trying to access a nonexistent quote index
    #[fail(display = "You haven't written that quote: {}.", index)]
    QuoteNotFound { index: usize },
    /// Thrown when trying to access an unrecorded book
    #[fail(display = "I haven't read {} yet.", book)]
    BookNotFound { book: String },
    /// Thrown when trying to access an unrecorded tag
    #[fail(display = "You haven't tagged anything as {} yet.", tag)]
    TagNotFound { tag: String },
    /// Thrown when no text is returned from an external editor
    #[fail(display = "Your editor of choice didn't work.")]
    EditorError,
    /// Thrown when argument is given empty input like '' or ' '
    #[fail(display = "Type something already!")]
    NoInputError,
    /// Catch-all for stuff that should never happen
    #[fail(display = "{}\nRedo from start.", message)]
    OutOfCheeseError { message: String },
    /// Thrown when explicit Y not received from user for destructive things
    #[fail(display = "{}\nDoing nothing.", message)]
    DoingNothing { message: String },
    /// Thrown when $HOME is not set
    #[fail(display = "$HOME not set")]
    Homeless,
    /// Thrown when badly formatted tsv file given for parsing
    #[fail(
        display = "I can't read {}. Make sure it has 'Quote', 'Book', and 'Author' columns and is tab-separated.",
        filename
    )]
    FileParseError { filename: String },
}
