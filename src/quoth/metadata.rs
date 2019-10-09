use failure::Error;
use path_abs::{FileRead, PathAbs, PathDir, PathFile, PathInfo, PathOps};
use serde_json;

use crate::config;

/// Stores `current_quote_index`, number of quotes, authors, books, and tags
#[derive(Serialize, Deserialize)]
pub struct Metadata {
    current_quote_index: usize,
    pub num_quotes: usize,
//    pub num_books: usize,
//    pub num_authors: usize,
//    pub num_tags: usize,
}

impl Metadata {
    /// Initialize
    fn create(quoth_dir: &PathDir) -> Result<Self, Error> {
        PathFile::create(quoth_dir.join(config::METADATA_PATH))?;
        let quoth_data = Metadata {
            current_quote_index: 0,
            num_quotes: 0,
//            num_books: 0,
//            num_authors: 0,
//            num_tags: 0,
        };
        quoth_data.write(quoth_dir)?;
        Ok(quoth_data)
    }

    /// Remove metadata file
    pub fn clear(quoth_dir: &PathDir) -> Result<(), Error> {
        let metadata_path = PathAbs::new(quoth_dir.join(config::METADATA_PATH))?;
        if metadata_path.exists() {
            PathFile::new(metadata_path)?.remove()?;
        }
        Ok(())
    }

    /// Change location of metadata file
    pub fn relocate(old_quoth_dir: &PathDir, new_quoth_dir: &PathDir) -> Result<(), Error> {
        PathFile::new(old_quoth_dir.join(config::METADATA_PATH))?
            .rename(PathFile::create(new_quoth_dir.join(config::METADATA_PATH))?)?;
        Ok(())
    }

    /// Read metadata from file (location in config)
    pub fn read(quoth_dir: &PathDir) -> Result<Self, Error> {
        if !PathAbs::new(quoth_dir.join(config::METADATA_PATH))?.exists() {
            Metadata::create(quoth_dir)
        } else {
            Ok(serde_json::from_reader(FileRead::open(PathFile::new(
                quoth_dir.join(config::METADATA_PATH),
            )?)?)?)
        }
    }

    /// Write (changed) metadata to file (location in config)
    pub fn write(&self, quoth_dir: &PathDir) -> Result<(), Error> {
        let index_file = PathFile::new(quoth_dir.join(config::METADATA_PATH))?;
        index_file.write_str(&serde_json::to_string(self)?)?;
        Ok(())
    }

    pub fn increment_quote_index(&mut self) {
        self.current_quote_index += 1;
    }

    pub fn get_quote_index(&self) -> usize {
        self.current_quote_index
    }

    pub fn increment_quotes(&mut self) {
        self.num_quotes += 1;
    }

    pub fn decrement_quotes(&mut self) {
        self.num_quotes -= 1;
    }

//    pub fn increment_books(&mut self) {
//        self.num_books += 1;
//    }
//
//    pub fn decrement_books(&mut self) {
//        self.num_books -= 1;
//    }
//
//    pub fn increment_authors(&mut self) {
//        self.num_authors += 1;
//    }
//
//    pub fn decrement_authors(&mut self) {
//        self.num_authors -= 1;
//    }
//
//    pub fn increment_tags(&mut self) {
//        self.num_tags += 1;
//    }
//
//    pub fn decrement_tags(&mut self) {
//        self.num_tags -= 1;
//    }
}
