use crate::config;
use crate::errors::QuothError;
use crate::quoth::metadata::Metadata;
use crate::quoth::quotes::Quote;
use crate::utils;
use failure::Error;
use path_abs::PathDir;
use sled;
use std::str;

/// Retrieve a `sled` tree from a given path
fn get_tree(path: &PathDir) -> Result<sled::Tree, Error> {
    let config = sled::ConfigBuilder::new()
        .path(path)
        .merge_operator(merge_index)
        .build();
    let tree = sled::Tree::start(config)?;
    Ok(tree)
}

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
    tree.set(
        key.to_vec(),
        utils::make_indices_string(&utils::insertion_sort(&utils::split_indices_usize(
            &tree.get(key)?.unwrap(),
        )?))?,
    )?;
    Ok(())
}

/// Stores linkage information between authors, books, tags and quotes, along with quoth metadata
pub struct Trees {
    /// Links authors to the quotes they've authored
    pub author_quote_tree: sled::Tree,
    /// Links authors to the books they've authored
    pub author_book_tree: sled::Tree,
    /// Links books to the quotes they contain
    pub book_quote_tree: sled::Tree,
    /// Links books to their authors
    pub book_author_tree: sled::Tree,
    /// Links tags to the quotes they're associated with
    pub tag_quote_tree: sled::Tree,
    /// Metadata about stored quotes
    pub metadata: Metadata,
}

impl Trees {
    /// Removes all `sled` trees
    pub fn clear(quoth_dir: &PathDir) -> Result<(), Error> {
        Metadata::clear(quoth_dir)?;
        PathDir::new(quoth_dir.join(config::AUTHOR_QUOTE_PATH))?.remove_all()?;
        PathDir::new(quoth_dir.join(config::AUTHOR_BOOK_PATH))?.remove_all()?;
        PathDir::new(quoth_dir.join(config::BOOK_QUOTE_PATH))?.remove_all()?;
        PathDir::new(quoth_dir.join(config::BOOK_AUTHOR_PATH))?.remove_all()?;
        PathDir::new(quoth_dir.join(config::TAG_QUOTE_PATH))?.remove_all()?;
        Ok(())
    }

    /// Copies a given tree to a new location
    fn copy_tree(old_tree: &sled::Tree, new_tree: &mut sled::Tree) -> Result<(), Error> {
        for key_value in old_tree {
            let (key, value) = key_value?;
            new_tree.set(key, value)?;
        }
        Ok(())
    }

    /// Changes the location of all `sled` trees and the metadata file
    pub fn relocate(old_quoth_dir: &PathDir, new_quoth_dir: &PathDir) -> Result<(), Error> {
        let old_trees = Trees::read(old_quoth_dir)?;
        Metadata::relocate(old_quoth_dir, new_quoth_dir)?;
        let mut new_trees = Trees::read(new_quoth_dir)?;
        Trees::copy_tree(
            &old_trees.author_quote_tree,
            &mut new_trees.author_quote_tree,
        )?;
        Trees::copy_tree(&old_trees.author_book_tree, &mut new_trees.author_book_tree)?;
        Trees::copy_tree(&old_trees.book_quote_tree, &mut new_trees.book_quote_tree)?;
        Trees::copy_tree(&old_trees.book_author_tree, &mut new_trees.book_author_tree)?;
        Trees::copy_tree(&old_trees.tag_quote_tree, &mut new_trees.tag_quote_tree)?;
        Trees::clear(old_quoth_dir)?;
        Ok(())
    }

    /// Reads `sled` trees and metadata file from the locations specified in config (makes new ones the first time)
    pub fn read(quoth_dir: &PathDir) -> Result<Self, Error> {
        Ok(Trees {
            author_quote_tree: get_tree(&PathDir::create_all(
                quoth_dir.join(config::AUTHOR_QUOTE_PATH),
            )?)?,
            author_book_tree: get_tree(&PathDir::create_all(
                quoth_dir.join(config::AUTHOR_BOOK_PATH),
            )?)?,
            book_quote_tree: get_tree(&PathDir::create_all(
                quoth_dir.join(config::BOOK_QUOTE_PATH),
            )?)?,
            book_author_tree: get_tree(&PathDir::create_all(
                quoth_dir.join(config::BOOK_AUTHOR_PATH),
            )?)?,
            tag_quote_tree: get_tree(&PathDir::create_all(
                quoth_dir.join(config::TAG_QUOTE_PATH),
            )?)?,
            metadata: Metadata::read(quoth_dir)?,
        })
    }

    /// Add a book to the trees and change metadata accordingly
    fn add_book(
        &mut self,
        author_key: &[u8],
        book_key: &[u8],
        index_key: &[u8],
    ) -> Result<(), Error> {
        self.author_book_tree
            .merge(author_key.to_vec(), book_key.to_vec())?;
        self.book_quote_tree
            .set(book_key.to_vec(), index_key.to_vec())?;
        self.book_author_tree
            .set(book_key.to_vec(), author_key.to_vec())?;
        self.metadata.increment_books();
        Ok(())
    }

    /// Add an author and a book to the trees and change metadata accordingly
    fn add_author_and_book(
        &mut self,
        author_key: &[u8],
        book_key: &[u8],
        index_key: &[u8],
    ) -> Result<(), Error> {
        let book = str::from_utf8(book_key)?.to_owned();
        if self.author_quote_tree.get(author_key)?.is_some() {
            self.author_quote_tree
                .merge(author_key.to_vec(), index_key.to_vec())?;
            let books =
                utils::split_values_string(&self.author_book_tree.get(author_key)?.ok_or(
                    QuothError::OutOfCheeseError {
                        message: "MELON MELON MELON".into(),
                    },
                )?)?;
            if books.contains(&book) {
                self.book_quote_tree
                    .merge(book_key.to_vec(), index_key.to_vec())?;
            } else {
                self.add_book(author_key, book_key, index_key)?;
            }
        } else {
            self.author_quote_tree
                .set(author_key.to_vec(), index_key.to_vec())?;
            self.add_book(author_key, book_key, index_key)?;
            self.metadata.increment_authors();
        }
        Ok(())
    }

    /// Add a tag to the trees and change metadata accordingly
    fn add_tag(&mut self, tag_key: &[u8], index_key: &[u8]) -> Result<(), Error> {
        if self.tag_quote_tree.get(tag_key)?.is_some() {
            self.tag_quote_tree
                .merge(tag_key.to_vec(), index_key.to_vec())?;
        } else {
            self.tag_quote_tree
                .set(tag_key.to_vec(), index_key.to_vec())?;
            self.metadata.increment_tags();
        }
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
            self.add_tag(tag_key, index_key)?;
        }
        self.metadata.write(quoth_dir)?;
        Ok(quote.index)
    }

    /// Delete a book
    fn delete_book(&mut self, book_key: &[u8]) -> Result<(), Error> {
        self.book_quote_tree.del(book_key)?;
        self.book_author_tree.del(book_key)?;
        self.metadata.decrement_books();
        Ok(())
    }

    /// Delete an author
    fn delete_author(&mut self, author_key: &[u8]) -> Result<(), Error> {
        self.author_quote_tree.del(author_key)?;
        let author = utils::u8_to_str(author_key)?;
        let books = utils::split_values_string(
            &self
                .author_book_tree
                .get(author_key)?
                .ok_or(QuothError::AuthorNotFound { author })?,
        )?;
        for book in books {
            self.delete_book(book.as_bytes())?;
        }
        self.author_book_tree.del(author_key)?;
        self.metadata.decrement_authors();
        Ok(())
    }

    /// Delete a tag
    fn delete_tag(&mut self, tag_key: &[u8]) -> Result<(), Error> {
        self.tag_quote_tree.del(tag_key)?;
        self.metadata.decrement_tags();
        Ok(())
    }

    /// Delete a quote index from the tag-quote tree
    fn delete_from_tag(&mut self, tag_key: &[u8], index: usize) -> Result<(), Error> {
        let tag = utils::u8_to_str(tag_key)?;
        let indices = utils::split_indices_usize(
            &self
                .tag_quote_tree
                .get(tag_key)?
                .ok_or(QuothError::TagNotFound { tag })?,
        )?;
        let mut new_indices = Vec::new();
        for index_i in indices {
            if index_i == index {
                continue;
            } else {
                new_indices.push(index_i);
            }
        }
        if new_indices.is_empty() {
            self.delete_tag(tag_key)?;
        } else {
            self.tag_quote_tree
                .set(tag_key.to_vec(), utils::make_indices_string(&new_indices)?)?;
        }
        Ok(())
    }

    /// Delete a quote index from the book-quote tree
    fn delete_from_book(&mut self, book_key: &[u8], index: usize) -> Result<(), Error> {
        let book = utils::u8_to_str(book_key)?;
        let indices = utils::split_indices_usize(
            &self
                .book_quote_tree
                .get(book_key)?
                .ok_or(QuothError::BookNotFound { book })?,
        )?;
        let mut new_indices = Vec::new();
        for index_i in indices {
            if index_i == index {
                continue;
            } else {
                new_indices.push(index_i);
            }
        }
        if new_indices.is_empty() {
            self.delete_book(book_key)?;
        } else {
            self.book_quote_tree
                .set(book_key.to_vec(), utils::make_indices_string(&new_indices)?)?;
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
        let indices = utils::split_indices_usize(
            &self
                .author_quote_tree
                .get(author_key)?
                .ok_or(QuothError::AuthorNotFound { author })?,
        )?;
        let mut new_indices = Vec::new();
        for index_i in indices {
            if index_i == index {
                continue;
            } else {
                new_indices.push(index_i);
            }
        }
        if new_indices.is_empty() {
            self.delete_author(author_key)?;
        } else {
            self.author_quote_tree.set(
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
        let old_author_key = old_quote.author.as_bytes();
        let old_book_key = old_quote.book.as_bytes();
        self.delete_from_author_and_book(old_author_key, old_book_key, index)?;
        for tag in old_quote.tags {
            self.delete_from_tag(tag.as_bytes(), index)?;
        }
        let author_key = new_quote.author.as_bytes();
        let book_key = new_quote.book.as_bytes();
        let index_key = index.to_string();
        let index_key = index_key.as_bytes();
        self.add_author_and_book(author_key, book_key, index_key)?;
        set_sorted(&mut self.author_quote_tree, author_key)?;
        set_sorted(&mut self.book_quote_tree, book_key)?;
        for tag in &new_quote.tags {
            let tag_key = tag.as_bytes();
            self.add_tag(tag_key, index_key)?;
            set_sorted(&mut self.tag_quote_tree, tag_key)?;
        }

        self.metadata.write(quoth_dir)?;
        Ok(())
    }

    /// Retrieve a given author's quotes
    pub fn get_author_quotes(&self, author: &str) -> Result<Vec<usize>, Error> {
        utils::split_indices_usize(
            &self
                .author_quote_tree
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
                .book_quote_tree
                .get(&utils::camel_case_phrase(book).as_bytes())?
                .ok_or(QuothError::BookNotFound {
                    book: book.to_owned(),
                })?,
        )
    }

    /// Retrieve quotes associated with a given tag
    pub fn get_tag_quotes(&self, tag: &str) -> Result<Vec<usize>, Error> {
        utils::split_indices_usize(&self.tag_quote_tree.get(tag.as_bytes())?.ok_or(
            QuothError::TagNotFound {
                tag: tag.to_owned(),
            },
        )?)
    }
}
