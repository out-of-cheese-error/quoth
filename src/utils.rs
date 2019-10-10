use anyhow::Error;
use chrono::{Date, Datelike, DateTime, MAX_DATE, MIN_DATE, Utc};
use chrono_english::{Dialect, parse_date_string};
use clap::ArgMatches;
use csv;
use dialoguer::{Editor, Input, theme};
use path_abs::PathFile;
use serde_json;
use termion::event::Key;
use termion::input::TermRead;

use std::collections::HashMap;
use std::io;
use std::str;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::config;
use crate::errors::QuothError;

pub const RAVEN: char = '\u{1313F}';

/// Capitalizes first letter of a word and lowercases the rest
fn camel_case_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase(),
        None => String::new(),
    }
}

/// Changes "caMel case Word" to "Camel Case Word"
pub fn camel_case_phrase(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| camel_case_word(word))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Splits input by comma
pub fn split_tags(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|word| word.trim().to_string())
        .collect::<Vec<String>>()
}

/// Converts an array of bytes to a string
pub fn u8_to_str(input: &[u8]) -> Result<String, Error> {
    Ok(str::from_utf8(input)?.to_owned())
}

/// Splits byte array by semicolon into strings
pub fn split_values_string(index_list: &[u8]) -> Result<Vec<String>, Error> {
    let index_list_string = str::from_utf8(index_list)?;
    Ok(index_list_string
        .split(str::from_utf8(&[config::SEMICOLON])?)
//        .filter(|s| s.len() > 0)
        .map(|s| s.to_string())
        .collect())
}

/// Splits byte array by semicolon into usize
pub fn split_indices_usize(index_list: &[u8]) -> Result<Vec<usize>, Error> {
    let index_list_string = str::from_utf8(index_list)?;
    Ok(index_list_string
        .split(str::from_utf8(&[config::SEMICOLON])?)
//        .filter(|s| s.len() > 0)
        .map(|word: &str| word.parse::<usize>())
        .collect::<Result<Vec<_>, _>>()?)
}

/// List of usize into semicolon-joined byte array
pub fn make_indices_string(index_list: &[usize]) -> Result<Vec<u8>, Error> {
    Ok(index_list
        .iter()
        .map(|index| index.to_string())
        .collect::<Vec<String>>()
        .join(str::from_utf8(&[config::SEMICOLON])?)
        .as_bytes()
        .to_vec())
}

pub fn parse_date(date_string: &str) -> Result<Date<Utc>, Error> {
    if date_string.to_ascii_lowercase() == "today" {
        Ok(Utc::now().date())
    } else {
        Ok(parse_date_string(date_string, Utc::now(), Dialect::Uk)?.date())
    }
}

/// Some(date) => date
/// None => minimum possible date
pub fn date_start(from_date: Option<DateTime<Utc>>) -> DateTime<Utc> {
    from_date.unwrap_or_else(|| MIN_DATE.and_hms(0, 0, 0))
}

/// Some(date) => date
/// None => maximum possible date
pub fn date_end(to_date: Option<DateTime<Utc>>) -> DateTime<Utc> {
    to_date.unwrap_or_else(|| MAX_DATE.and_hms(23, 59, 59))
}

/// Gets input from external editor, optionally displays default text in editor
pub fn external_editor_input(default: Option<&str>) -> Result<String, Error> {
    match Editor::new().edit(default.unwrap_or(""))? {
        Some(input) => Ok(input),
        None => Err(QuothError::EditorError.into()),
    }
}

/// Takes user input from terminal, optionally has a default and optionally displays it.
pub fn user_input(
    message: &str,
    default: Option<&str>,
    show_default: bool,
) -> Result<String, Error> {
    match default {
        Some(default) => Ok(Input::with_theme(&theme::ColorfulTheme::default())
            .with_prompt(message)
            .default(default.to_owned())
            .show_default(show_default)
            .interact()?
            .trim()
            .to_owned()),
        None => Ok(
            Input::<String>::with_theme(&theme::ColorfulTheme::default())
                .with_prompt(message)
                .interact()?
                .trim()
                .to_owned(),
        ),
    }
}

/// Extracts value of a given argument from matches if present
pub fn get_argument_value<'a>(
    name: &str,
    matches: &'a ArgMatches<'a>,
) -> Result<Option<&'a str>, Error> {
    match matches.value_of(name) {
        Some(value) => {
            if value.trim().is_empty() {
                Err(QuothError::NoInputError.into())
            } else {
                Ok(Some(value.trim()))
            }
        }
        None => Ok(None),
    }
}

/// Sorts array using insertion sort (good for almost sorted arrays)
pub fn insertion_sort(array: &[usize]) -> Vec<usize> {
    let mut output_array = array.to_vec().clone();
    for slot in 1..array.len() {
        let value = output_array[slot];
        let mut test_slot = slot - 1;
        while output_array[test_slot] > value {
            output_array[test_slot + 1] = output_array[test_slot];
            if test_slot == 0 {
                break;
            }
            test_slot -= 1;
        }
        output_array[test_slot + 1] = value;
    }
    output_array
}

pub fn get_months(min_date: Date<Utc>, max_date: Date<Utc>) -> Vec<Date<Utc>> {
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

/// Reads quote database (downloaded from https://github.com/ShivaliGoel/Quotes-500K) and saves it as
/// a JSON file of authors mapped to all their quotes.
pub fn read_quotes_database(
    full_database_file: &str,
    output_database_file: &str,
) -> Result<(), Error> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b',')
        .from_path(&full_database_file)?;
    let mut quote_db = HashMap::new();
    for result in reader.records() {
        let record = result?;
        let quote = record.get(0);
        let author_book = record.get(1);
        if let (Some(quote), Some(author_book)) = (quote, author_book) {
            let author_book = author_book.split(',').collect::<Vec<_>>();
            // Filters out book-less quotes
            if author_book.len() >= 2 {
                quote_db
                    .entry(author_book[0].to_owned())
                    .or_insert_with(Vec::new)
                    .push(quote.to_owned());
            }
        }
    }
    let output_database_file = PathFile::create(output_database_file)?;
    output_database_file.write_str(&serde_json::to_string(&quote_db)?)?;
    Ok(())
}
