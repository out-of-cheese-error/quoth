#![feature(inner_deref)]
#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;

mod config;
mod errors;
mod quoth;
mod utils;

use crate::quoth::Quoth;
use clap::App;
use failure::Error;

fn main() -> Result<(), Error> {
    let yaml = load_yaml!("quoth.yml");
    let matches = App::from_yaml(yaml).get_matches();
    if let Err(err) = Quoth::start(matches) {
        println!("{}", err);
    }
    Ok(())
}
