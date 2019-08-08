use std::io;

use crate::quoth::get_quoth_dir;
use crate::quoth::metadata::Metadata;
use crate::quoth::quotes::Quote;
use chrono::{Date, Datelike, Utc, MAX_DATE, MIN_DATE};
use failure::Error;
use path_abs::{PathAbs, PathDir, PathFile};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use termion::event::Key;
use termion::input::MouseTerminal;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use textwrap::termwidth;
use tui::backend::TermionBackend;
use tui::layout::{Alignment, Constraint, Direction, Layout};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{BarChart, Block, Borders, Paragraph, Row, Sparkline, Table, Text, Widget};
use tui::Terminal;
use crate::utils;

pub enum Event<I> {
    Input(I),
    Tick,
}

/// A small event handler that wraps termion input and tick events. Each event
/// type is handled in its own thread and returned to a common `Receiver`
pub struct Events {
    rx: mpsc::Receiver<Event<Key>>,
    input_handle: thread::JoinHandle<()>,
    tick_handle: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub exit_key: Key,
    pub tick_rate: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            exit_key: Key::Char('q'),
            tick_rate: Duration::from_millis(250),
        }
    }
}

impl Events {
    pub fn new() -> Events {
        Events::with_config(Config::default())
    }

    pub fn with_config(config: Config) -> Events {
        let (tx, rx) = mpsc::channel();
        let input_handle = {
            let tx = tx.clone();
            thread::spawn(move || {
                let stdin = io::stdin();
                for evt in stdin.keys() {
                    match evt {
                        Ok(key) => {
                            if let Err(_) = tx.send(Event::Input(key)) {
                                return;
                            }
                            if key == config.exit_key {
                                return;
                            }
                        }
                        Err(_) => {}
                    }
                }
            })
        };
        let tick_handle = {
            let tx = tx.clone();
            thread::spawn(move || {
                let tx = tx.clone();
                loop {
                    tx.send(Event::Tick).unwrap();
                    thread::sleep(config.tick_rate);
                }
            })
        };
        Events {
            rx,
            input_handle,
            tick_handle,
        }
    }

    pub fn next(&self) -> Result<Event<Key>, mpsc::RecvError> {
        self.rx.recv()
    }
}

pub struct Stats {
    pub book_counts: Vec<(String, u64)>,
    pub quote_counts: Vec<(String, u64)>,
    pub max_books: u64,
    pub max_quotes: u64,
    num_bars: usize,
    pub start_index_bar: usize,
    pub end_index_bar: usize,
    pub author_table: Vec<Vec<String>>,
    pub start_index_table: usize,
    pub end_index_table: usize,
    num_rows: usize,
    pub metadata: Metadata,
}

fn format_date(date: &Date<Utc>) -> String {
    let year = date.year().to_string().chars().skip(2).collect::<String>();
    format!("{}-{}", date.month(), year)
}

impl Stats {
    fn get_counts_per_time(
        quoth_dir: &PathDir,
    ) -> Result<(Vec<(String, u64)>, Vec<(String, u64)>, u64, u64), Error> {
        let mut book_dates = HashMap::new();
        let mut quote_counts = HashMap::new();
        for quote in Quote::list_in_date_range(
            MIN_DATE.and_hms(0, 0, 0),
            MAX_DATE.and_hms(23, 59, 59),
            quoth_dir,
        )? {
            *quote_counts
                .entry(quote.date.date().with_day(1).unwrap())
                .or_insert(0) += 1;
            book_dates.insert(quote.book, quote.date.date().with_day(1).unwrap());
        }
        let mut book_counts = HashMap::new();
        for (_, month) in book_dates {
            *book_counts.entry(month).or_insert(0) += 1;
        }
        let max_books = *book_counts.iter().max_by(|a, b| a.1.cmp(b.1)).unwrap().1;
        let max_quotes = *quote_counts.iter().max_by(|a, b| a.1.cmp(b.1)).unwrap().1;
        let mut months: Vec<_> = quote_counts.keys().collect();
        let (min_date, max_date) = (
            **months.iter().min().unwrap(),
            **months.iter().max().unwrap(),
        );
        let (min_year, min_month) = (min_date.year(), min_date.month());
        let (max_year, max_month) = (max_date.year(), max_date.month());
        let mut months = Vec::with_capacity((max_year - min_year) as usize * 12);
        let mut date = Utc::now().date();
        for month in min_month..=12 {
            months.push(
                date.with_year(min_year)
                    .unwrap()
                    .with_month(month)
                    .unwrap()
                    .with_day(1)
                    .unwrap(),
            );
        }
        for year in min_year..max_year {
            for month in 1..=12 {
                months.push(
                    date.with_year(year)
                        .unwrap()
                        .with_month(month)
                        .unwrap()
                        .with_day(1)
                        .unwrap(),
                );
            }
        }
        for month in 1..=max_month {
            months.push(
                date.with_year(max_year)
                    .unwrap()
                    .with_month(month)
                    .unwrap()
                    .with_day(1)
                    .unwrap(),
            );
        }

        let book_counts: Vec<(String, u64)> = months
            .iter()
            .map(|m| (format_date(m), *(book_counts.get(m).unwrap_or(&0))))
            .collect();
        let quote_counts: Vec<(String, u64)> = months
            .iter()
            .map(|m| (format_date(m), *(quote_counts.get(m).unwrap_or(&0))))
            .collect();
        Ok((book_counts, quote_counts, max_books, max_quotes))
    }

    fn get_author_table(quoth_dir: &PathDir) -> Result<Vec<Vec<String>>, Error> {
        let mut author_quotes = HashMap::new();
        let mut author_books = HashMap::new();
        for quote in Quote::list_in_date_range(
            MIN_DATE.and_hms(0, 0, 0),
            MAX_DATE.and_hms(23, 59, 59),
            quoth_dir,
        )? {
            *author_quotes.entry(quote.author.clone()).or_insert(0usize) += 1;
            author_books
                .entry(quote.author)
                .or_insert(HashSet::new())
                .insert(quote.book);
        }
        let mut author_quotes = author_quotes.into_iter().collect::<Vec<_>>();
        author_quotes.sort();
        let author_table = author_quotes
            .into_iter()
            .map(|(a, q)| {
                let num_books = author_books.get(&a).unwrap_or(&HashSet::new()).len();
                vec![a, num_books.to_string(), q.to_string()]
            })
            .collect();
        Ok(author_table)
    }

    pub fn from_quoth(quoth_dir: &PathDir, num_bars: usize, num_rows: usize) -> Result<Self, Error> {
        let (book_counts, quote_counts, max_books, max_quotes) =
            Stats::get_counts_per_time(quoth_dir)?;
        let num_bars = num_bars.min(quote_counts.len());
        let author_table = Stats::get_author_table(quoth_dir)?;
        let num_rows = num_rows.min(author_table.len());
        Ok(Stats {
            book_counts,
            quote_counts,
            max_books,
            max_quotes,
            start_index_bar: 0,
            end_index_bar: num_bars,
            num_bars,
            author_table,
            start_index_table: 0,
            end_index_table: num_rows,
            num_rows,
            metadata: Metadata::read(quoth_dir)?,
        })
    }

    pub fn update(&mut self, key: Key) {
        match key {
            Key::Right => {
                self.start_index_bar += 1;
                self.end_index_bar += 1;
                if self.end_index_bar >= self.quote_counts.len() {
                    self.end_index_bar = self.quote_counts.len();
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
                if self.end_index_table >= self.author_table.len() {
                    self.end_index_table = self.author_table.len();
                }
                if self.end_index_table - self.start_index_table < self.num_rows {
                    self.start_index_table = self.end_index_table - self.num_rows;
                }
            }
            _ => (),
        }
    }
}

//pub fn display_stats() -> Result<(), failure::Error> {
//    // Terminal initialization
//    let stdout = io::stdout().into_raw_mode()?;
//    let stdout = MouseTerminal::from(stdout);
//    let stdout = AlternateScreen::from(stdout);
//    let backend = TermionBackend::new(stdout);
//    let mut terminal = Terminal::new(backend)?;
//    terminal.hide_cursor()?;
//
//    // Setup event handlers
//    let events = Events::new();
//
//    // App
//    let quoth_dir = get_quoth_dir()?;
//    let bar_width = 5;
//    let num_rows = (terminal.size()?.height / 5 - 4) as usize;
//    let mut quoth_stats = Stats::from_quoth(quoth_dir, termwidth() / bar_width, num_rows)?;
//    loop {
//        terminal.draw(|mut f| {
//            let chunks = Layout::default()
//                .direction(Direction::Vertical)
//                .margin(2)
//                .constraints(
//                    [
//                        Constraint::Percentage(40),
//                        Constraint::Percentage(40),
//                        Constraint::Percentage(20),
//                    ]
//                    .as_ref(),
//                )
//                .split(f.size());
//
//            // Quote Stats
//            BarChart::default()
//                .block(Block::default().title("Quotes").borders(Borders::ALL))
//                .data(
//                    &quoth_stats.quote_counts
//                        [quoth_stats.start_index_bar..quoth_stats.end_index_bar]
//                        .iter()
//                        .map(|(m, x)| (m.as_str(), *x))
//                        .collect::<Vec<_>>(),
//                )
//                .bar_width(bar_width as u16)
//                .max(quoth_stats.max_quotes)
//                .style(Style::default().fg(Color::Gray))
//                .value_style(Style::default().bg(Color::Black))
//                .render(&mut f, chunks[1]);
//
//
//            // Book Stats
//            BarChart::default()
//                .block(Block::default().title("Books").borders(Borders::ALL))
//                .data(
//                    &quoth_stats.book_counts
//                        [quoth_stats.start_index_bar..quoth_stats.end_index_bar]
//                        .iter()
//                        .map(|(m, x)| (m.as_str(), *x))
//                        .collect::<Vec<_>>(),
//                )
//                .bar_width(bar_width as u16)
//                .max(quoth_stats.max_books)
//                .style(Style::default().fg(Color::Cyan))
//                .value_style(Style::default().bg(Color::Black))
//                .render(&mut f, chunks[0]);
//
//
//            {
//                let chunks = Layout::default()
//                    .direction(Direction::Horizontal)
//                    .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
//                    .split(chunks[2]);
//
//
//                // Author Stats
//                let row_style = Style::default().fg(Color::White);
//                let header_style = Style::default().fg(Color::Blue).modifier(Modifier::BOLD);
//                Table::new(
//                    vec!["Author", "Books", "Quotes"].into_iter(),
//                    quoth_stats.author_table
//                        [quoth_stats.start_index_table..quoth_stats.end_index_table]
//                        .iter()
//                        .map(|row| Row::StyledData(row.into_iter(), row_style)),
//                )
//                    .header_style(header_style)
//                .block(Block::default().title("Authors").borders(Borders::ALL))
//                .widths(&[25, 5, 5])
//                .render(&mut f, chunks[0]);
//
//
//                // Total Stats
//                Paragraph::new(vec![
//                    Text::styled(&format!("{}\n", utils::RAVEN), Style::default().modifier(Modifier::DIM)),
//                    Text::raw(&format!("# Quotes {}\n", quoth_stats.metadata.num_quotes) ),
//                    Text::styled(&format!("# Books {}\n", quoth_stats.metadata.num_books), Style::default().fg(Color::Cyan)),
//                    Text::styled(&format!("# Authors {}\n", quoth_stats.metadata.num_authors), Style::default().fg(Color::Blue)),
//                    Text::styled(&format!("# Tags {}\n", quoth_stats.metadata.num_tags), Style::default().modifier(Modifier::DIM)),
//                ].iter())
//                    .block(Block::default().title("Total").borders(Borders::ALL))
//                    .alignment(Alignment::Center)
//                    .render(&mut f, chunks[1]);
//            }
//        })?;
//
//        match events.next()? {
//            Event::Input(input) => {
//                if input == Key::Char('q') {
//                    break;
//                } else {
//                    quoth_stats.update(input);
//                }
//            }
//            Event::Tick => (),
//        }
//    }
//
//    Ok(())
//}
