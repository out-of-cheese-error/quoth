#![feature(transpose_result)]
#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;

use csv;
use serde_json;
use sled;
mod config;
mod errors;
mod quoth;
mod utils;

use crate::quoth::Quoth;
use clap::App;

fn main() {
    let yaml = load_yaml!("quoth.yml");
    let matches = App::from_yaml(yaml).get_matches();
    if let Err(err) = Quoth::start(matches) {
        println!("{}", err);
    }
}
