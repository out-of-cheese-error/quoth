use anyhow::Error;
use bincode;
use chrono::{DateTime, Utc};
use console::{Alignment, pad_str, style};
use path_abs::{FileRead, PathFile};
use serde_json;
use textwrap::{termwidth, Wrapper};

use crate::utils;

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

    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        Ok(bincode::serialize(&self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Ok(bincode::deserialize(bytes)?)
    }

    /// Read quotes from a JSON file and return consumable iterator
    pub fn read_from_file(
        json_file: &PathFile,
    ) -> Result<impl Iterator<Item = serde_json::Result<Quote>>, Error> {
        Ok(serde_json::Deserializer::from_reader(FileRead::open(json_file)?).into_iter::<Self>())
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
