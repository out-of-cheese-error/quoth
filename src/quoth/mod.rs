use std::collections::HashMap;
use std::io;

use anyhow::{Context, Error};
use chrono::{Date, Datelike, DateTime, MAX_DATE, MIN_DATE, Utc};
use clap::{App, ArgMatches, Shell};
use csv;
use dirs;
use path_abs::{PathAbs, PathDir, PathFile, PathInfo, PathOps};
use rand::Rng;
use regex::Regex;
use serde_json;
use termion::event::Key;
use termion::input::MouseTerminal;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use textwrap::termwidth;
use tui::backend::TermionBackend;
use tui::layout::{Alignment, Constraint, Direction, Layout};
use tui::style::{Color, Modifier, Style};
use tui::Terminal;
use tui::widgets::{BarChart, Block, Borders, Paragraph, Row, Table, Text, Widget};

use crate::config;
use crate::errors::QuothError;
use crate::quoth::database::Trees;
use crate::quoth::quotes::{Quote, TSVQuote};
use crate::utils;

mod database;
mod quotes;

/// Makes config file (default ~/quoth.txt) with a single line containing the location of the quoth directory (default ~/.quoth)
fn make_quoth_config_file() -> Result<(), Error> {
    match dirs::home_dir() {
        Some(home_dir) => {
            let config_file = PathFile::create(PathDir::new(&home_dir)?.join(config::CONFIG_PATH))?;
            config_file.write_str(
                &PathDir::new(home_dir)?
                    .join(config::QUOTH_DIR_DEFAULT)
                    .to_str()
                    .unwrap(),
            )?;
            Ok(())
        }
        None => Err(QuothError::Homeless.into()),
    }
}

/// Reads config file to get location of the quoth directory
pub fn get_quoth_dir() -> Result<PathDir, Error> {
    match dirs::home_dir() {
        Some(home_dir) => {
            let config_file = PathAbs::new(PathDir::new(home_dir)?.join(config::CONFIG_PATH))?;
            if !config_file.exists() {
                make_quoth_config_file()?;
            }
            let quoth_dir_string = PathFile::new(config_file)?.read_string()?;
            Ok(PathDir::create_all(quoth_dir_string.trim())?)
        }
        None => Err(QuothError::Homeless.into()),
    }
}

/// Changes the location of the quoth directory
fn change_quoth_dir(new_dir: &str) -> Result<(), Error> {
    match dirs::home_dir() {
        Some(home_dir) => {
            let config_file = PathFile::create(PathDir::new(home_dir)?.join(config::CONFIG_PATH))?;
            config_file.write_str(new_dir)?;
            Ok(())
        }
        None => Err(QuothError::Homeless.into()),
    }
}

/// Stores
/// - the location of the quoth directory
/// - argument parsing information from `clap`
/// - the `sled` databases storing linkage information between authors, books, tags, and quotes
pub struct Quoth<'a> {
    quoth_dir: &'a PathDir,
    matches: ArgMatches<'a>,
    trees: Trees,
}

/// Stores (author, book, tag, date) filters parsed from command-line arguments to restrict the quotes to look at
struct Filters<'a> {
    author: Option<&'a str>,
    book: Option<&'a str>,
    tag: Option<&'a str>,
    from_date: Option<DateTime<Utc>>,
    to_date: Option<DateTime<Utc>>,
}

impl<'a> Filters<'a> {
    /// Parses filters (on author, book, tag, date) from command-line arguments
    fn get_filters(matches: &'a ArgMatches<'a>) -> Result<Filters<'a>, Error> {
        let on_date = utils::get_argument_value("on", matches)?;
        let from_date = if on_date.is_some() {
            on_date
        } else {
            utils::get_argument_value("from", matches)?
        }
        .map(|date| utils::parse_date(date))
        .transpose()?
        .map(|date| date.and_hms(0, 0, 0));
        let to_date = if on_date.is_some() {
            on_date
        } else {
            utils::get_argument_value("to", &matches)?
        }
        .map(|date| utils::parse_date(date))
        .transpose()?
        .map(|date| date.and_hms(23, 59, 59));

        let (author, book, tag) = (
            utils::get_argument_value("author", matches)?,
            utils::get_argument_value("book", matches)?,
            utils::get_argument_value("tag", matches)?,
        );
        Ok(Filters {
            author,
            book,
            tag,
            from_date,
            to_date,
        })
    }
}

impl<'a> Quoth<'a> {
    /// Initialize program
    pub fn start(matches: ArgMatches<'a>) -> Result<(), Error> {
        let quoth_dir = &get_quoth_dir()?;
        let trees = Trees::read(quoth_dir)?;
        let mut quoth = Quoth {
            quoth_dir,
            matches,
            trees,
        };
        quoth.run()
    }

    /// Parses command-line arguments to decide which sub-command to run
    fn run(&mut self) -> Result<(), Error> {
        if self.matches.is_present("delete") {
            self.delete_quote()
        } else if self.matches.is_present("show") {
            self.show_quote()
        } else if self.matches.is_present("change") {
            self.change_quote()
        } else {
            match self.matches.subcommand() {
                ("stats", Some(matches)) => self.stats(matches),
                ("config", Some(matches)) => self.config(matches),
                ("import", Some(matches)) => {
                    for quote in self.import(matches)? {
                        self.trees.add_quote(&quote)?;
                    }
                    Ok(())
                }
                ("export", Some(matches)) => self.export(matches),
                ("list", Some(matches)) => self.list(matches),
                ("search", Some(matches)) => self.search(matches),
                ("random", Some(matches)) => self.random(matches),
                _ => self.quoth(),
            }
        }
    }

    /// Generates shell completions
    fn completions(&self, matches: &ArgMatches<'a>) -> Result<(), Error> {
        let shell = utils::get_argument_value("completions", matches)?.ok_or(
            QuothError::OutOfCheeseError {
                message: "Argument shell not used".into(),
            },
        )?;
        let yaml = load_yaml!("../quoth.yml");
        let mut app = App::from_yaml(yaml);
        app.gen_completions_to("quoth", shell.parse::<Shell>().unwrap(), &mut io::stdout());
        Ok(())
    }

    /// Clears all quoth data or changes the quote directory or generates shell completions
    fn config(&self, matches: &ArgMatches<'a>) -> Result<(), Error> {
        if matches.is_present("clear") {
            self.clear()
        } else if matches.is_present("dir") {
            self.relocate(matches)
        } else if matches.is_present("completions") {
            self.completions(matches)
        } else {
            Err(QuothError::OutOfCheeseError {
                message: "Unknown/No config argument".into(),
            }
            .into())
        }
    }

    /// Adds a new quote
    fn quoth(&mut self) -> Result<(), Error> {
        let quote = Quote::from_user(self.trees.get_quote_index()? + 1, None)?;
        println!(
            "Added quote #{}",
            self.trees.add_quote(&quote)?
        );
        Ok(())
    }

    /// Changes a quote at a particular index
    fn change_quote(&mut self) -> Result<(), Error> {
        let index = utils::get_argument_value("change", &self.matches)?
            .ok_or(QuothError::OutOfCheeseError {
                message: "Argument change not used".into(),
            })?
            .parse::<usize>()?;
        let old_quote = self.trees.get_quote(index)?;
        let new_quote = Quote::from_user(index, Some(old_quote))?;
        self.trees.change_quote(index, &new_quote)?;
        println!("Quote #{} changed", index);
        Ok(())
    }

    /// Filters a list of quotes by given author/book/tag/date
    fn filter_quotes(&self, filters: &Filters<'_>) -> Result<Vec<Quote>, Error> {
        let from_date = utils::date_start(filters.from_date);
        let to_date = utils::date_end(filters.to_date);
        let quotes: Option<Vec<_>> = match (filters.author, filters.book) {
            (Some(author), None) => Some(self.trees.get_quotes(
                &self.trees.get_author_quotes(author)?,
            )?),
            (None, Some(book)) => Some(self.trees.get_quotes(
                &self.trees.get_book_quotes(book)?,
            )?),
            (Some(_), Some(_)) => {
                return Err(QuothError::OutOfCheeseError {
                    message: "Can't filter by both author and book".into(),
                }
                .into())
            }
            (None, None) => None,
        };
        match (filters.tag, quotes) {
            (Some(tag), Some(quotes)) => Ok(quotes
                .into_iter()
                .filter(|quote| quote.has_tag(tag) && quote.in_date_range(from_date, to_date))
                .collect()),
            (Some(tag), None) => Quote::filter_in_date_range(
                self.trees.get_quotes(&self.trees.get_tag_quotes(tag)?)?,
                from_date,
                to_date,
            ),
            (None, Some(quotes)) => Quote::filter_in_date_range(quotes, from_date, to_date),
            (None, None) => self.trees.list_quotes_in_date_range(from_date, to_date),
        }
    }

    /// Shows a quote matching a given index
    fn show_quote(&self) -> Result<(), Error> {
        let index =
            utils::get_argument_value("show", &self.matches)?.ok_or(QuothError::OutOfCheeseError {
                message: "Argument index not used".into(),
            })?.parse::<usize>().with_context(|| format!("Given index is not a number"))?;
        self.trees.get_quote(index)?.pretty_print();
        Ok(())
    }

    /// Lists quotes (optionally filtered)
    fn list(&self, matches: &ArgMatches<'a>) -> Result<(), Error> {
        let filters = Filters::get_filters(matches)?;
        let quotes = self.filter_quotes(&filters)?;
        for quote in &quotes {
            quote.pretty_print();
        }
        Ok(())
    }

    /// Displays a random quote (optionally filtered)
    fn random(&self, matches: &ArgMatches<'a>) -> Result<(), Error> {
        let filters = Filters::get_filters(matches)?;
        let quotes = self.filter_quotes(&filters)?;
        quotes[rand::thread_rng().gen_range(0, quotes.len())].pretty_print();
        Ok(())
    }

    /// Searches the list of quotes (optionally filtered) for a pattern and displays quotes matching it
    fn search(&self, matches: &ArgMatches<'a>) -> Result<(), Error> {
        let pattern =
            utils::get_argument_value("pattern", matches)?.ok_or(QuothError::OutOfCheeseError {
                message: "Argument pattern not used".into(),
            })?;
        let pattern = Regex::new(&format!(
            r"(?imxs){}",
            pattern.split_whitespace().collect::<Vec<_>>().join(".+")
        ))?;
        let filters = Filters::get_filters(matches)?;
        let quotes = self.filter_quotes(&filters)?;
        for quote in &quotes {
            if pattern.is_match(&quote.to_string()) {
                quote.pretty_print();
            }
        }
        Ok(())
    }

    /// Clears all quoth data
    fn clear(&self) -> Result<(), Error> {
        let mut sure_delete;
        loop {
            sure_delete = utils::user_input("Clear all quoth data Y/N?", Some("N"), true)?
                .to_ascii_uppercase();
            if sure_delete == "Y" || sure_delete == "N" {
                break;
            }
        }
        if sure_delete == "Y" {
            Trees::clear(self.quoth_dir)?;
            Ok(())
        } else {
            Err(QuothError::DoingNothing {
                message: "I'm a coward.".into(),
            }
            .into())
        }
    }

    /// Changes quoth directory
    fn relocate(&self, matches: &ArgMatches<'a>) -> Result<(), Error> {
        let new_dir =
            utils::get_argument_value("dir", matches)?.ok_or(QuothError::OutOfCheeseError {
                message: "Argument dir not used".into(),
            })?;
        let new_dir_path = PathDir::create_all(new_dir)?;
        if &new_dir_path == self.quoth_dir {
            return Err(QuothError::DoingNothing {
                message: "Same as old dir.".into(),
            }
            .into());
        }
        Trees::relocate(self.quoth_dir, &new_dir_path)?;
        change_quoth_dir(new_dir)?;
        let mut delete_old_dir;
        loop {
            delete_old_dir = utils::user_input("Delete old directory Y/N?", Some("N"), true)?
                .to_ascii_uppercase();
            if delete_old_dir == "Y" || delete_old_dir == "N" {
                break;
            }
        }
        if delete_old_dir == "Y" {
            self.quoth_dir.clone().remove_all()?;
            Ok(())
        } else {
            Err(QuothError::DoingNothing {
                message: "I'm a coward.".into(),
            }
            .into())
        }
    }

    /// Deletes a quote at a particular index
    fn delete_quote(&mut self) -> Result<(), Error> {
        let index = utils::get_argument_value("delete", &self.matches)?.ok_or(
            QuothError::OutOfCheeseError {
                message: "Argument delete not used".into(),
            },
        )?;
        let mut sure_delete;
        loop {
            sure_delete =
                utils::user_input(&format!("Delete quote #{} Y/N?", index), Some("N"), true)?
                    .to_ascii_uppercase();
            if sure_delete == "Y" || sure_delete == "N" {
                break;
            }
        }
        if sure_delete == "Y" {
            self.trees
                .delete_quote(index.parse::<usize>()?)?;
            println!("Quote #{} deleted", index);
            Ok(())
        } else {
            Err(QuothError::DoingNothing {
                message: "I'm a coward.".into(),
            }
            .into())
        }
    }

    /// Saves (optionally filtered) quotes to a TSV file
    fn export(&self, matches: &ArgMatches<'a>) -> Result<(), Error> {
        let filters = Filters::get_filters(matches)?;
        let mut writer = csv::WriterBuilder::new()
            .delimiter(b'\t')
            .from_path(PathFile::create(
                utils::get_argument_value("filename", matches)?.ok_or(
                    QuothError::OutOfCheeseError {
                        message: "Argument filename not used".into(),
                    },
                )?,
            )?)?;
        let quotes = self.filter_quotes(&filters)?;
        for quote in quotes {
            writer.serialize(TSVQuote::from(quote))?;
        }
        writer.flush()?;
        Ok(())
    }

    /// Parses quotes from a JSON/TSV file and adds them to quoth
    fn import(&self, matches: &ArgMatches<'a>) -> Result<Vec<Quote>, Error> {
        if matches.is_present("json") {
            let json_file = PathFile::new(utils::get_argument_value("json", matches)?.ok_or(
                QuothError::OutOfCheeseError {
                    message: "Argument json not used".into(),
                },
            )?)?;
            let quotes: Result<Vec<Quote>, serde_json::Error> =
                Quote::read_from_file(&json_file)?.collect();
            Ok(quotes?)
        } else if matches.is_present("tsv") {
            let tsv_file = PathFile::new(utils::get_argument_value("tsv", matches)?.ok_or(
                QuothError::OutOfCheeseError {
                    message: "Argument tsv not used".into(),
                },
            )?)?;
            let mut reader = csv::ReaderBuilder::new()
                .delimiter(b'\t')
                .from_path(&tsv_file)?;
            let quoth_headers: HashMap<&str, i32> = [
                ("BOOK", 0),
                ("AUTHOR", 1),
                ("TAGS", 2),
                ("DATE", 3),
                ("QUOTE", 4),
            ]
            .iter()
            .cloned()
            .collect();
            let header_indices: Vec<_> = reader
                .headers()?
                .into_iter()
                .map(|h| quoth_headers.get(h.to_ascii_uppercase().as_str()))
                .collect();
            let mut quotes = Vec::new();
            let mut quote_index = self.trees.get_quote_index()? + 1;
            if [0, 1, 4].iter().all(|x| header_indices.contains(&Some(x))) {
                for record in reader.records() {
                    let mut quote_data = ("", "", "", Utc::now(), String::new());
                    let record = record?;
                    for (entry, index) in record.into_iter().zip(header_indices.iter()) {
                        if let Some(i) = index {
                            match i {
                                0 => quote_data.0 = entry,
                                1 => quote_data.1 = entry,
                                2 => quote_data.2 = entry,
                                3 => quote_data.3 = utils::parse_date(entry)?.and_hms(0, 0, 0),
                                4 => quote_data.4 = entry.into(),
                                _ => {
                                    return Err(QuothError::OutOfCheeseError {
                                        message: "Please Reinstall Universe And Reboot".into(),
                                    }
                                    .into())
                                }
                            }
                        }
                    }
                    quotes.push(Quote::new(
                        quote_index,
                        quote_data.0,
                        quote_data.1,
                        quote_data.2,
                        quote_data.3,
                        quote_data.4,
                    ));
                    quote_index += 1;
                }
                Ok(quotes)
            } else {
                Err(QuothError::FileParseError {
                    filename: tsv_file
                        .to_str()
                        .ok_or(QuothError::OutOfCheeseError {
                            message: "Bad filename".into(),
                        })?
                        .into(),
                }
                .into())
            }
        } else {
            Err(QuothError::OutOfCheeseError {
                message: "Can only handle JSON or TSV input".into(),
            }
            .into())
        }
    }

    /// Uses termion and tui to display a dashboard with 4 components
    /// 1. Number of quotes written per month as a bar chart
    /// 2. Number of books read per month as a bar chart
    /// 3. A table of the number of books and quotes corresponding to each author
    /// 4. Total numbers of quotes, books, authors, and tags recorded in quoth
    /// Use arrow keys to scroll the bar charts and the table
    /// q to quit display
    fn stats(&self, matches: &ArgMatches<'a>) -> Result<(), Error> {
        let from_date = utils::get_argument_value("from", matches)?
            .map(|date| utils::parse_date(date))
            .transpose()?
            .map(|date| date.and_hms(0, 0, 0))
            .unwrap_or_else(|| MIN_DATE.and_hms(0, 0, 0));
        let to_date = utils::get_argument_value("to", &matches)?
            .map(|date| utils::parse_date(date))
            .transpose()?
            .map(|date| date.and_hms(23, 59, 59))
            .unwrap_or_else(|| MAX_DATE.and_hms(23, 59, 59));

        //         Terminal initialization
        let stdout = io::stdout().into_raw_mode()?;
        let stdout = MouseTerminal::from(stdout);
        let stdout = AlternateScreen::from(stdout);
        let backend = TermionBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;

        //         Setup event handlers
        let events = utils::Events::new();

        //         Get counts
        let bar_width = 5;
        let num_rows = (terminal.size()?.height / 5 - 4) as usize;
        let num_bars = termwidth() / bar_width;

        let (quote_counts, book_counts) =
            self.trees.get_quote_and_book_counts_per_month(from_date, to_date)?;
        let (max_books, max_quotes) = (
            *book_counts.values().max().unwrap(),
            *quote_counts.values().max().unwrap(),
        );
        let months: Vec<_> = quote_counts.keys().collect();
        let (min_date, max_date) = (
            **months.iter().min().unwrap(),
            **months.iter().max().unwrap(),
        );
        let months = utils::get_months(min_date, max_date);

        fn format_date(date: Date<Utc>) -> String {
            let year = date.year().to_string().chars().skip(2).collect::<String>();
            format!("{}-{}", date.month(), year)
        }

        let book_counts: Vec<(String, u64)> = months
            .iter()
            .map(|m| (format_date(*m), *(book_counts.get(m).unwrap_or(&0))))
            .collect();
        let quote_counts: Vec<(String, u64)> = months
            .iter()
            .map(|m| (format_date(*m), *(quote_counts.get(m).unwrap_or(&0))))
            .collect();
        let num_bars = num_bars.min(quote_counts.len());
        let author_table = self.trees.get_author_counts()?;
        let mut author_table: Vec<Vec<String>> = author_table
            .into_iter()
            .map(|(a, (b, q))| vec![a, b.to_string(), q.to_string()])
            .collect();
        author_table.sort();
        let num_rows = num_rows.min(author_table.len());
        let mut scrollers = Scrollers {
            start_index_bar: 0,
            end_index_bar: num_bars,
            max_index_bar: quote_counts.len(),
            num_bars,
            start_index_table: 0,
            end_index_table: num_rows,
            max_index_table: author_table.len(),
            num_rows,
        };
        let (num_quotes, num_books, num_authors, num_tags) = (
            self.trees.quote_tree()?.len(),
            self.trees.book_quote_tree()?.len(),
            self.trees.author_quote_tree()?.len(),
            self.trees.tag_quote_tree()?.len(),
        );
        loop {
            terminal.draw(|mut f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(2)
                    .constraints(
                        [
                            Constraint::Percentage(40),
                            Constraint::Percentage(40),
                            Constraint::Percentage(20),
                        ]
                        .as_ref(),
                    )
                    .split(f.size());

                // Quote Stats
                BarChart::default()
                    .block(Block::default().title("Quotes").borders(Borders::ALL))
                    .data(
                        &quote_counts[scrollers.start_index_bar..scrollers.end_index_bar]
                            .iter()
                            .map(|(m, x)| (m.as_str(), *x))
                            .collect::<Vec<_>>(),
                    )
                    .bar_width(bar_width as u16)
                    .max(max_quotes)
                    .style(Style::default().fg(Color::Gray))
                    .value_style(Style::default().bg(Color::Black))
                    .render(&mut f, chunks[0]);

                // Book Stats
                BarChart::default()
                    .block(Block::default().title("Books").borders(Borders::ALL))
                    .data(
                        &book_counts[scrollers.start_index_bar..scrollers.end_index_bar]
                            .iter()
                            .map(|(m, x)| (m.as_str(), *x))
                            .collect::<Vec<_>>(),
                    )
                    .bar_width(bar_width as u16)
                    .max(max_books)
                    .style(Style::default().fg(Color::Cyan))
                    .value_style(Style::default().bg(Color::Black))
                    .render(&mut f, chunks[1]);

                {
                    let chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(
                            [Constraint::Percentage(70), Constraint::Percentage(30)].as_ref(),
                        )
                        .split(chunks[2]);

                    // Author Stats
                    let row_style = Style::default().fg(Color::White);
                    let header_style = Style::default().fg(Color::Blue).modifier(Modifier::BOLD);
                    Table::new(
                        vec!["Author", "Books", "Quotes"].into_iter(),
                        author_table[scrollers.start_index_table..scrollers.end_index_table]
                            .iter()
                            .map(|row| Row::StyledData(row.iter(), row_style)),
                    )
                    .header_style(header_style)
                    .block(Block::default().title("Authors").borders(Borders::ALL))
                    .widths(&[25, 5, 5])
                    .render(&mut f, chunks[0]);

                    // Total Stats
                    Paragraph::new(
                        vec![
                            Text::styled(
                                &format!("{}\n", utils::RAVEN),
                                Style::default().modifier(Modifier::DIM),
                            ),
                            Text::raw(&format!("# Quotes {}\n", num_quotes)),
                            Text::styled(
                                &format!("# Books {}\n", num_books),
                                Style::default().fg(Color::Cyan),
                            ),
                            Text::styled(
                                &format!("# Authors {}\n", num_authors),
                                Style::default().fg(Color::Blue),
                            ),
                            Text::styled(
                                &format!("# Tags {}\n", num_tags),
                                Style::default().modifier(Modifier::DIM),
                            ),
                            Text::raw("\nScroll: arrow keys\nQuit: q\n"),
                        ]
                        .iter(),
                    )
                    .block(Block::default().title("Total").borders(Borders::ALL))
                    .alignment(Alignment::Center)
                    .render(&mut f, chunks[1]);
                }
            })?;

            if let utils::Event::Input(input) = events.next()? {
                if input == Key::Char('q') {
                    break;
                } else {
                    scrollers.update(input);
                }
            }
        }
        Ok(())
    }
}

struct Scrollers {
    num_bars: usize,
    start_index_bar: usize,
    end_index_bar: usize,
    max_index_bar: usize,
    start_index_table: usize,
    end_index_table: usize,
    max_index_table: usize,
    num_rows: usize,
}

impl Scrollers {
    fn update(&mut self, key: Key) {
        match key {
            Key::Right => {
                self.start_index_bar += 1;
                self.end_index_bar += 1;
                if self.end_index_bar >= self.max_index_bar {
                    self.end_index_bar = self.max_index_bar;
                }
                if self.end_index_bar - self.start_index_bar < self.num_bars {
                    self.start_index_bar = self.end_index_bar - self.num_bars;
                }
            }
            Key::Left => {
                if self.start_index_bar > 0 {
                    self.start_index_bar -= 1;
                    self.end_index_bar -= 1;
                }
            }
            Key::Up => {
                if self.start_index_table > 0 {
                    self.start_index_table -= 1;
                    self.end_index_table -= 1;
                }
            }
            Key::Down => {
                self.start_index_table += 1;
                self.end_index_table += 1;
                if self.end_index_table >= self.max_index_table {
                    self.end_index_table = self.max_index_table;
                }
                if self.end_index_table - self.start_index_table < self.num_rows {
                    self.start_index_table = self.end_index_table - self.num_rows;
                }
            }
            _ => (),
        }
    }
}

