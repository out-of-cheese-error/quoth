use crate::config;
use crate::errors::QuothError;
use chrono::{Date, DateTime, Utc, MAX_DATE, MIN_DATE};
use chrono_english::{parse_date_string, Dialect};
use clap::ArgMatches;
use dialoguer::{Editor, Input};
use failure::Error;
use std::str;

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
        .map(|s| s.to_string())
        .collect())
}

/// Splits byte array by semicolon into usize
pub fn split_indices_usize(index_list: &[u8]) -> Result<Vec<usize>, Error> {
    let index_list_string = str::from_utf8(index_list)?;
    Ok(index_list_string
        .split(str::from_utf8(&[config::SEMICOLON])?)
        .map(|word: &str| word.parse::<usize>())
        .scan((), |_, x| x.ok())
        .collect())
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
    Ok(parse_date_string(date_string, Utc::now(), Dialect::Uk)?.date())
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
        Some(d) => Ok(Input::new(message)
            .default(d)
            .show_default(show_default)
            .interact()?
            .trim()
            .to_owned()),
        None => Ok(Input::new(message).interact()?.trim().to_owned()),
    }
}

/// Extracts value of a given argument from matches if present
pub fn get_argument_value<'a>(
    name: &str,
    matches: &'a ArgMatches<'a>,
) -> Result<Option<&'a str>, Error> {
    if matches.is_present(name) {
        let value = matches.value_of(name).ok_or(QuothError::OutOfCheeseError {
            message: format!("No argument value for {}", name),
        })?;
        if value.trim().is_empty() {
            Err(QuothError::NoInputError.into())
        } else {
            Ok(Some(value.trim()))
        }
    } else {
        Ok(None)
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
