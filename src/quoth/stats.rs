#![allow(dead_code)]
use std::io;
use crate::quoth::metadata::Metadata;
use crate::quoth::quotes::Quote;
use chrono::{Date, Datelike, Utc, MAX_DATE, MIN_DATE};
use failure::Error;
use path_abs::PathDir;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use termion::event::Key;
use termion::input::TermRead;
use crate::errors::QuothError;

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
                    if let Ok(key) = evt {
                        if tx.send(Event::Input(key)).is_err() {
                            return;
                        }
                        if key == config.exit_key {
                            return;
                        }
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

fn format_date(date: Date<Utc>) -> String {
    let year = date.year().to_string().chars().skip(2).collect::<String>();
    format!("{}-{}", date.month(), year)
}

impl Stats {
    fn get_months(min_date: Date<Utc>, max_date: Date<Utc>) -> Vec<Date<Utc>> {
        let (min_year, min_month) = (min_date.year(), min_date.month());
        let (max_year, max_month) = (max_date.year(), max_date.month());
        let mut months = Vec::with_capacity((max_year - min_year) as usize * 12);
        let date = Utc::now().date();
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
        months
    }

    fn get_counts_per_month(
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
                .entry(quote.date.date().with_day(1).ok_or(QuothError::OutOfCheeseError {message: "This month doesn't have a first day".into()})?)
                .or_insert(0) += 1;
            book_dates.insert(quote.book, quote.date.date().with_day(1).ok_or(QuothError::OutOfCheeseError {message: "This month doesn't have a first day".into()})?);
        }
        let mut book_counts = HashMap::new();
        for (_, month) in book_dates {
            *book_counts.entry(month).or_insert(0) += 1;
        }
        let max_books = *book_counts.iter().max_by(|a, b| a.1.cmp(b.1)).unwrap().1;
        let max_quotes = *quote_counts.iter().max_by(|a, b| a.1.cmp(b.1)).unwrap().1;
        let months: Vec<_> = quote_counts.keys().collect();
        let (min_date, max_date) = (
            **months.iter().min().unwrap(),
            **months.iter().max().unwrap(),
        );
        let months = Stats::get_months(min_date, max_date);
        let book_counts: Vec<(String, u64)> = months
            .iter()
            .map(|m| (format_date(*m), *(book_counts.get(m).unwrap_or(&0))))
            .collect();
        let quote_counts: Vec<(String, u64)> = months
            .iter()
            .map(|m| (format_date(*m), *(quote_counts.get(m).unwrap_or(&0))))
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
            Stats::get_counts_per_month(quoth_dir)?;
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

