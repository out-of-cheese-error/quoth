use std::collections::HashMap;
use std::str;

use failure::Error;
use path_abs::{PathDir, PathOps};
use sled;

use crate::config;
use crate::errors::QuothError;
use crate::quoth::metadata::Metadata;
use crate::quoth::quotes::Quote;
use crate::utils;

/// If key exists, add value to existing values - join with a semicolon
fn merge_index(_key: &[u8], old_indices: Option<&[u8]>, new_index: &[u8]) -> Option<Vec<u8>> {
    let mut ret = old_indices
        .map(|old| old.to_vec())
        .unwrap_or_else(|| vec![]);
    ret.extend_from_slice(&[config::SEMICOLON]);
    ret.extend_from_slice(new_index);
    Some(ret)
}

/// Sort indices and set key value to sorted indices
fn set_sorted(tree: &mut sled::Tree, key: &[u8]) -> Result<(), Error> {
    tree.insert(
        key.to_vec(),
        utils::make_indices_string(&utils::insertion_sort(&utils::split_indices_usize(
            &tree.get(key)?.unwrap(),
        )?))?,
    )?;
    Ok(())
}

/// Stores linkage information between authors, books, tags and quotes, along with quoth metadata
pub struct Trees {
    pub db: sled::Db,
    pub metadata: Metadata,
}

impl Trees {
    /// Removes all `sled` trees
    pub fn clear(quoth_dir: &PathDir) -> Result<(), Error> {
        Metadata::clear(quoth_dir)?;
        PathDir::new(quoth_dir.join(config::DB_PATH))?.remove_all()?;
        Ok(())
    }

    pub fn author_quote_tree(&self) -> Result<sled::Tree, Error> {
        Ok(self.db.open_tree("author_quote")?)
    }

    pub fn author_book_tree(&self) -> Result<sled::Tree, Error> {
        Ok(self.db.open_tree("author_book")?)
    }

    pub fn book_quote_tree(&self) -> Result<sled::Tree, Error> {
        Ok(self.db.open_tree("book_quote")?)
    }

    pub fn book_author_tree(&self) -> Result<sled::Tree, Error> {
        Ok(self.db.open_tree("book_author")?)
    }

    pub fn tag_quote_tree(&self) -> Result<sled::Tree, Error> {
        Ok(self.db.open_tree("tag_quote")?)
    }

    /// Changes the location of all `sled` trees and the metadata file
    pub fn relocate(old_quoth_dir: &PathDir, new_quoth_dir: &PathDir) -> Result<(), Error> {
        let old_trees = Trees::read(old_quoth_dir)?.db.export();
        Metadata::relocate(old_quoth_dir, new_quoth_dir)?;
        let new_trees = Trees::read(new_quoth_dir)?;
        new_trees.db.import(old_trees);
        Trees::clear(old_quoth_dir)?;
        Ok(())
    }

    /// Reads `sled` trees and metadata file from the locations specified in config (makes new ones the first time)
    pub fn read(quoth_dir: &PathDir) -> Result<Self, Error> {
        let config = sled::ConfigBuilder::new()
            .path(&PathDir::create_all(quoth_dir.join(config::DB_PATH))?)
            .build();
        let db = sled::Db::start(config)?;
        Ok(Trees {
            db,
            metadata: Metadata::read(quoth_dir)?,
        })
    }

    /// Add a book to the trees
    fn add_book(
        &mut self,
        author_key: &[u8],
        book_key: &[u8],
        index_key: &[u8],
    ) -> Result<(), Error> {
        self.author_book_tree()?
            .merge(author_key.to_vec(), book_key.to_vec())?;
        self.book_quote_tree()?
            .merge(book_key.to_vec(), index_key.to_vec())?;
        self.book_author_tree()?
            .merge(book_key.to_vec(), author_key.to_vec())?;
        Ok(())
    }

    /// Add an author and a book to the trees
    fn add_author_and_book(
        &mut self,
        author_key: &[u8],
        book_key: &[u8],
        index_key: &[u8],
    ) -> Result<(), Error> {
        self.author_quote_tree()?.merge(author_key.to_vec(), index_key.to_vec())?;
        self.add_book(author_key, book_key, index_key)?;
        Ok(())
    }

    /// Add a Quote (with all attached data) to the trees and change metadata accordingly
    pub fn add_quote(&mut self, quote: &Quote, quoth_dir: &PathDir) -> Result<usize, Error> {
        quote.add(&mut self.metadata, quoth_dir)?;
        let author_key = quote.author.as_bytes();
        let book_key = quote.book.as_bytes();
        let index_key = quote.index.to_string();
        let index_key = index_key.as_bytes();
        self.add_author_and_book(author_key, book_key, index_key)?;
        for tag in &quote.tags {
            let tag_key = tag.as_bytes();
            self.tag_quote_tree()?.merge(tag_key.to_vec(), index_key.to_vec())?;
        }
        self.metadata.write(quoth_dir)?;
        Ok(quote.index)
    }

    /// Delete a book
    fn delete_book(&mut self, book_key: &[u8]) -> Result<(), Error> {
        self.book_quote_tree()?.remove(book_key)?;
        self.book_author_tree()?.remove(book_key)?;
        Ok(())
    }

    /// Delete an author
    fn delete_author(&mut self, author_key: &[u8]) -> Result<(), Error> {
        self.author_quote_tree()?.remove(author_key)?;
        let author = utils::u8_to_str(author_key)?;
        let books = utils::split_values_string(
            &self
                .author_book_tree()?
                .get(author_key)?
                .ok_or(QuothError::AuthorNotFound { author })?,
        )?;
        for book in books {
            self.delete_book(book.as_bytes())?;
        }
        self.author_book_tree()?.remove(author_key)?;
        Ok(())
    }

    /// Delete a quote index from the tag-quote tree
    fn delete_from_tag(&mut self, tag_key: &[u8], index: usize) -> Result<(), Error> {
        let tag = utils::u8_to_str(tag_key)?;
        let new_indices: Vec<_> = utils::split_indices_usize(
            &self
                .tag_quote_tree()?
                .get(tag_key)?
                .ok_or(QuothError::TagNotFound { tag })?,
        )?
        .into_iter()
        .filter(|index_i| *index_i != index)
        .collect();
        if new_indices.is_empty() {
            self.tag_quote_tree()?.remove(tag_key)?;
        } else {
            self.tag_quote_tree()?
                .insert(tag_key.to_vec(), utils::make_indices_string(&new_indices)?)?;
        }
        Ok(())
    }

    /// Delete a quote index from the book-quote tree
    fn delete_from_book(&mut self, book_key: &[u8], index: usize) -> Result<(), Error> {
        let book = utils::u8_to_str(book_key)?;
        let new_indices: Vec<_> = utils::split_indices_usize(
            &self
                .book_quote_tree()?
                .get(book_key)?
                .ok_or(QuothError::BookNotFound { book })?,
        )?
        .into_iter()
        .filter(|index_i| *index_i != index)
        .collect();
        if new_indices.is_empty() {
            self.book_quote_tree()?.remove(book_key)?;
            self.book_author_tree()?.remove(book_key)?;
        } else {
            self.book_quote_tree()?
                .insert(book_key.to_vec(), utils::make_indices_string(&new_indices)?)?;
        }
        Ok(())
    }

    /// Delete a quote index from the author and book trees
    fn delete_from_author_and_book(
        &mut self,
        author_key: &[u8],
        book_key: &[u8],
        index: usize,
    ) -> Result<(), Error> {
        let author = utils::u8_to_str(author_key)?;
        let new_indices: Vec<_> = utils::split_indices_usize(
            &self
                .author_quote_tree()?
                .get(author_key)?
                .ok_or(QuothError::AuthorNotFound { author })?,
        )?
        .into_iter()
        .filter(|index_i| *index_i != index)
        .collect();
        if new_indices.is_empty() {
            self.delete_author(author_key)?;
        } else {
            self.author_quote_tree()?.insert(
                author_key.to_vec(),
                utils::make_indices_string(&new_indices)?,
            )?;
            self.delete_from_book(book_key, index)?;
        }
        Ok(())
    }

    /// Delete a quote (and all associated data) from the trees and metadata
    pub fn delete_quote(&mut self, index: usize, quoth_dir: &PathDir) -> Result<(), Error> {
        let quote = Quote::delete(index, &mut self.metadata, quoth_dir)?
            .ok_or(QuothError::QuoteNotFound { index })?;
        let author_key = quote.author.as_bytes();
        let book_key = quote.book.as_bytes();
        self.delete_from_author_and_book(author_key, book_key, index)?;
        for tag in quote.tags {
            self.delete_from_tag(tag.as_bytes(), index)?;
        }
        self.metadata.write(quoth_dir)?;
        Ok(())
    }

    /// Change a stored quote's information
    pub fn change_quote(
        &mut self,
        index: usize,
        new_quote: &Quote,
        quoth_dir: &PathDir,
    ) -> Result<(), Error> {
        let old_quote = Quote::change(index, new_quote, quoth_dir)?
            .ok_or(QuothError::QuoteNotFound { index })?;
        let (old_author_key, old_book_key) =
            (old_quote.author.as_bytes(), old_quote.book.as_bytes());
        self.delete_from_author_and_book(old_author_key, old_book_key, index)?;
        for tag in old_quote.tags {
            self.delete_from_tag(tag.as_bytes(), index)?;
        }
        let (author_key, book_key) = (new_quote.author.as_bytes(), new_quote.book.as_bytes());
        let index_key = index.to_string();
        let index_key = index_key.as_bytes();
        self.add_author_and_book(author_key, book_key, index_key)?;
        set_sorted(&mut self.author_quote_tree()?, author_key)?;
        set_sorted(&mut self.book_quote_tree()?, book_key)?;
        for tag in &new_quote.tags {
            let tag_key = tag.as_bytes();
            self.tag_quote_tree()?.merge(tag_key.to_vec(), index_key.to_vec())?;
            set_sorted(&mut self.tag_quote_tree()?, tag_key)?;
        }
        Ok(())
    }

    /// Retrieve a given author's quotes
    pub fn get_author_quotes(&self, author: &str) -> Result<Vec<usize>, Error> {
        utils::split_indices_usize(
            &self
                .author_quote_tree()?
                .get(&utils::camel_case_phrase(author).as_bytes())?
                .ok_or(QuothError::AuthorNotFound {
                    author: author.to_owned(),
                })?,
        )
    }

    /// Retrieve quotes from a given book
    pub fn get_book_quotes(&self, book: &str) -> Result<Vec<usize>, Error> {
        utils::split_indices_usize(
            &self
                .book_quote_tree()?
                .get(&utils::camel_case_phrase(book).as_bytes())?
                .ok_or(QuothError::BookNotFound {
                    book: book.to_owned(),
                })?,
        )
    }

    /// Retrieve quotes associated with a given tag
    pub fn get_tag_quotes(&self, tag: &str) -> Result<Vec<usize>, Error> {
        utils::split_indices_usize(&self.tag_quote_tree()?.get(tag.as_bytes())?.ok_or(
            QuothError::TagNotFound {
                tag: tag.to_owned(),
            },
        )?)
    }

    /// Get number of books and number of quotes per author for all authors stored
    pub fn get_author_counts(&self) -> Result<HashMap<String, (u64, u64)>, Error> {
        let author_books: HashMap<String, u64> = self
            .author_book_tree()?
            .iter()
            .map(|item| {
                item.map_err(|_| QuothError::OutOfCheeseError {
                    message: "sled PageCache Error".into(),
                })
                .and_then(|(a, books)| {
                    match (utils::u8_to_str(&a), utils::split_values_string(&books)) {
                        (Ok(a), Ok(books)) => Ok((a, books.len() as u64)),
                        _ => Err(QuothError::OutOfCheeseError {
                            message: "Corrupt author_book_tree".into(),
                        }),
                    }
                })
            })
            .collect::<Result<_, _>>()?;
        let author_quotes: HashMap<String, u64> = self
            .author_quote_tree()?
            .iter()
            .map(|item| {
                item.map_err(|_| QuothError::OutOfCheeseError {
                    message: "sled PageCache Error".into(),
                })
                .and_then(|(a, quotes)| {
                    match (utils::u8_to_str(&a), utils::split_indices_usize(&quotes)) {
                        (Ok(a), Ok(quotes)) => Ok((a, quotes.len() as u64)),
                        _ => Err(QuothError::OutOfCheeseError {
                            message: "Corrupt author_quote_tree".into(),
                        }),
                    }
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(author_quotes
            .into_iter()
            .map(|(a, q)| {
                let b = *author_books.get(&a).unwrap_or(&0);
                (a, (b, q))
            })
            .collect())
    }
}
