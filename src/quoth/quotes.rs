use crate::config;
use crate::errors::QuothError;
use crate::quoth::metadata::Metadata;
use crate::utils;
//use crate::utils::OptionDeref;
use chrono::{DateTime, Utc};
use console::{pad_str, style, Alignment};
use failure::Error;
use path_abs::{FileRead, PathDir, PathFile};
use serde_json;
use textwrap::{termwidth, Wrapper};

/// Stores information about a quote
#[derive(Serialize, Deserialize, Debug)]
pub struct Quote {
    /// Quote index, used to retrieve and modify a quote
    pub index: usize,
    /// Title of the quote's book
    pub book: String,
    /// Name of the quote's author
    pub author: String,
    /// Tags attached to a quote
    pub tags: Vec<String>,
    /// Date of recording the quote
    pub date: DateTime<Utc>,
    /// Quote text
    pub quote: String,
}

/// Stores quote information as Strings for writing to a file
#[derive(Serialize, Deserialize, Debug)]
pub struct TSVQuote {
    /// Quote index, used to retrieve and modify a quote
    index: usize,
    /// Title of the quote's book
    book: String,
    /// Name of the quote's author
    author: String,
    /// Tags attached to a quote
    tags: String,
    /// Date of recording the quote
    date: String,
    /// Quote text
    quote: String,
}
impl From<Quote> for TSVQuote {
    fn from(quote: Quote) -> Self {
        TSVQuote {
            index: quote.index,
            book: quote.book,
            author: quote.author,
            tags: quote.tags.join(","),
            date: quote.date.date().format("%Y-%m-%d").to_string(),
            quote: quote.quote,
        }
    }
}

impl ToString for Quote {
    fn to_string(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}",
            self.quote,
            self.author,
            self.book,
            self.tags.join(",")
        )
    }
}

impl Quote {
    /// New quote
    pub fn new(
        index: usize,
        title: &str,
        author: &str,
        tags: &str,
        date: DateTime<Utc>,
        quote: String,
    ) -> Self {
        Quote {
            index,
            book: utils::camel_case_phrase(title),
            author: utils::camel_case_phrase(author),
            tags: utils::split_tags(tags),
            date,
            quote,
        }
    }

    pub fn from_user(index: usize, default_quote: Option<Quote>) -> Result<Quote, Error> {
        let default_quote = match default_quote {
            Some(q) => Some(TSVQuote::from(q)),
            None => None,
        };
        let (default_title, default_author, default_tags, default_date, default_text) =
            match default_quote {
                Some(q) => (
                    Some(q.book),
                    Some(q.author),
                    Some(q.tags),
                    Some(q.date),
                    Some(q.quote),
                ),
                None => (None, None, None, None, None),
            };
        let title = utils::user_input("Book Title", default_title.as_deref(), false)?;
        let author = utils::user_input("Author", default_author.as_deref(), false)?;
        let tags = utils::user_input("Tags (comma separated)", default_tags.as_deref(), false)?;
        let date = match default_date {
            Some(_) => {
                utils::parse_date(&utils::user_input("Date", default_date.as_deref(), true)?)?
                    .and_hms(0, 0, 0)
            }
            None => Utc::now(),
        };
        let mut quote_text = utils::user_input(
            "Quote (<RET> to edit in external editor)",
            Some("\n"),
            false,
        )?;
        if quote_text.is_empty() {
            quote_text = utils::external_editor_input(default_text.as_deref())?;
        }
        Ok(Quote::new(index, &title, &author, &tags, date, quote_text))
    }

    /// Write quote to Quotes JSON file
    fn write(&self, quoth_dir: &PathDir) -> Result<(), Error> {
        let quote_json = serde_json::to_string(self)?;
        let quote_file = PathFile::create(quoth_dir.join(config::QUOTE_PATH))?;
        quote_file.append_str(&quote_json)?;
        Ok(())
    }

    /// Read quotes from a JSON file and return consumable iterator
    pub fn read_from_file(
        json_file: &PathFile,
    ) -> Result<impl Iterator<Item = serde_json::Result<Quote>>, Error> {
        Ok(serde_json::Deserializer::from_reader(FileRead::read(json_file)?).into_iter::<Self>())
    }

    /// Read quotes from Quote file stored in config
    fn read(quoth_dir: &PathDir) -> Result<impl Iterator<Item = serde_json::Result<Quote>>, Error> {
        Quote::read_from_file(&PathFile::new(quoth_dir.join(config::QUOTE_PATH))?)
    }

    /// Add a quote: write it to the quote file, increment the `current_quote_index` and the number of quotes
    pub fn add(&self, metadata: &mut Metadata, quoth_dir: &PathDir) -> Result<(), Error> {
        self.write(quoth_dir)?;
        metadata.increment_quote_index();
        metadata.increment_quotes();
        Ok(())
    }

    /// Retrieve a quote by index
    pub fn retrieve(index: usize, quoth_dir: &PathDir) -> Result<Self, Error> {
        let quote_stream = Quote::read(quoth_dir)?;
        for quote in quote_stream {
            let quote = quote?;
            if quote.index == index {
                return Ok(quote);
            }
        }
        Err(QuothError::QuoteNotFound { index }.into())
    }

    /// Retrieve many quotes given indices
    pub fn retrieve_many(indices: &[usize], quoth_dir: &PathDir) -> Result<Vec<Self>, Error> {
        let mut indices = indices.to_vec().into_iter().peekable();
        let mut quote_stream = Quote::read(quoth_dir)?;
        let mut quotes = Vec::new();
        while let Some(index) = indices.peek() {
            let quote = quote_stream
                .next()
                .ok_or(QuothError::QuoteNotFound { index: *index })??;
            if quote.index == *index {
                quotes.push(quote);
                indices.next().ok_or(QuothError::OutOfCheeseError {
                    message: "no more indices".into(),
                })?;
            }
        }
        Ok(quotes)
    }

    /// Remove quote file (Clears quotes)
    pub fn clear(quoth_dir: &PathDir) -> Result<(), Error> {
        PathFile::new(quoth_dir.join(config::QUOTE_PATH))?.remove()?;
        Ok(())
    }

    /// Change location of quote file
    pub fn relocate(old_quoth_dir: &PathDir, new_quoth_dir: &PathDir) -> Result<(), Error> {
        PathFile::new(old_quoth_dir.join(config::QUOTE_PATH))?
            .rename(PathFile::create(new_quoth_dir.join(config::QUOTE_PATH))?)?;
        Ok(())
    }

    /// Delete a quote by index
    pub fn delete(
        index: usize,
        metadata: &mut Metadata,
        quoth_dir: &PathDir,
    ) -> Result<Option<Self>, Error> {
        let quote_stream = Quote::read(quoth_dir)?;
        let mut quote = None;
        Quote::clear(quoth_dir)?;
        for quote_i in quote_stream {
            let quote_i = quote_i?;
            if quote_i.index == index {
                quote = Some(quote_i);
            } else {
                quote_i.write(quoth_dir)?;
            }
        }
        if quote.is_some() {
            metadata.decrement_quotes();
        }
        Ok(quote)
    }

    /// Change a quote by index
    pub fn change(
        index: usize,
        new_quote: &Self,
        quoth_dir: &PathDir,
    ) -> Result<Option<Self>, Error> {
        let quote_stream = Quote::read(quoth_dir)?;
        let mut old_quote = None;
        Quote::clear(quoth_dir)?;
        for quote_i in quote_stream {
            let quote_i = quote_i?;
            if quote_i.index == index {
                old_quote = Some(quote_i);
                new_quote.write(quoth_dir)?;
            } else {
                quote_i.write(quoth_dir)?;
            }
        }
        Ok(old_quote)
    }

    /// List quotes in date range
    pub fn list_in_date_range(
        from_date: DateTime<Utc>,
        to_date: DateTime<Utc>,
        quoth_dir: &PathDir,
    ) -> Result<Vec<Quote>, Error> {
        let quotes: Result<Vec<Quote>, serde_json::Error> = Quote::read(quoth_dir)?.collect();
        Ok(quotes?
            .into_iter()
            .filter(|quote| quote.in_date_range(from_date, to_date))
            .collect())
    }

    /// Filters quotes in date range
    pub fn filter_in_date_range(
        quotes: Vec<Quote>,
        from_date: DateTime<Utc>,
        to_date: DateTime<Utc>,
    ) -> Result<Vec<Quote>, Error> {
        Ok(quotes
            .into_iter()
            .filter(|quote| quote.in_date_range(from_date, to_date))
            .collect())
    }

    /// Checks if a quote was recorded within a date range
    pub fn in_date_range(&self, from_date: DateTime<Utc>, to_date: DateTime<Utc>) -> bool {
        from_date <= self.date && self.date < to_date
    }

    /// Check if a quote has a particular tag associated with it
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(&tag.into())
    }

    /// Display a quote in the terminal prettily
    pub fn pretty_print(&self) {
        let width = termwidth() - 4;
        let wrapper = Wrapper::new(width)
            .initial_indent("  ")
            .subsequent_indent("  ");
        print!(
            "{}",
            style(pad_str(
                &utils::RAVEN.to_string(),
                width,
                Alignment::Center,
                None
            ))
            .dim()
        );
        for line in self.quote.split('\n') {
            println!(
                "\n{}",
                pad_str(&wrapper.fill(line), width, Alignment::Center, None)
            );
        }
        println!(
            "{}",
            style(pad_str(
                &format!("--#{}--", self.index),
                width,
                Alignment::Center,
                None
            ))
            .dim()
        );
        println!(
            "{}",
            style(pad_str(&self.author, width - 4, Alignment::Right, None)).blue()
        );
        println!(
            "{}",
            style(pad_str(&self.book, width - 4, Alignment::Right, None))
                .cyan()
                .italic()
        );
        println!(
            "{}\n",
            style(pad_str(
                &self.tags.join(", "),
                width - 4,
                Alignment::Right,
                None
            ))
            .dim()
        )
    }
}
