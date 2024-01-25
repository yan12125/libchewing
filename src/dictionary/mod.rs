//! Systems and user phrase dictionaries.

use std::{
    any::Any,
    borrow::Borrow,
    cmp::Ordering,
    collections::HashMap,
    fmt::{Debug, Display},
    path::Path,
};

use thiserror::Error;

use crate::zhuyin::{Syllable, SyllableSlice};

pub use self::cdb::{CdbDictionary, CdbDictionaryBuilder, CdbDictionaryError};
pub use layered::LayeredDictionary;
pub use loader::{SystemDictionaryLoader, UserDictionaryLoader};
#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteDictionary, SqliteDictionaryBuilder, SqliteDictionaryError};
pub use trie::{TrieDictionary, TrieDictionaryBuilder, TrieDictionaryStatistics};

mod cdb;
mod kv;
mod layered;
mod loader;
#[cfg(feature = "sqlite")]
mod sqlite;
mod trie;

/// The error type which is returned from updating a dictionary.
#[derive(Error, Debug)]
#[error("update dictionary failed")]
pub struct DictionaryUpdateError {
    /// TODO: doc
    /// TODO: change this to anyhow::Error?
    #[from]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

/// The error type which is returned from building or updating a dictionary.
#[derive(Error, Debug)]
#[error("found duplicated phrases")]
pub struct DuplicatePhraseError;

/// A collection of metadata of a dictionary.
///
/// The dictionary version and copyright information can be used in
/// configuration application.
///
/// # Examples
///
/// ```no_run
/// # use std::collections::HashMap;
/// # use chewing::dictionary::Dictionary;
/// # let dictionary = HashMap::new();
/// let about = dictionary.about();
/// assert_eq!("libchewing default", about.name.unwrap());
/// assert_eq!("Copyright (c) 2022 libchewing Core Team", about.copyright.unwrap());
/// assert_eq!("LGPL-2.1-or-later", about.license.unwrap());
/// assert_eq!("init_database 0.5.1", about.software.unwrap());
/// ```
#[derive(Debug, Clone, Default)]
pub struct DictionaryInfo {
    /// The name of the dictionary.
    pub name: Option<String>,
    /// The copyright information of the dictionary.
    ///
    /// It's recommended to include the copyright holders' names and email
    /// addresses, separated by semicolons.
    pub copyright: Option<String>,
    /// The license information of the dictionary.
    ///
    /// It's recommended to use the [SPDX license identifier](https://spdx.org/licenses/).
    pub license: Option<String>,
    /// The version of the dictionary.
    ///
    /// It's recommended to use the commit hash or revision if the dictionary is
    /// managed in a source control repository.
    pub version: Option<String>,
    /// The name of the software used to generate the dictionary.
    ///
    /// It's recommended to include the name and the version number.
    pub software: Option<String>,
}

/// A type containing a phrase string and its frequency.
///
/// # Examples
///
/// A `Phrase` can be created from/to a tuple.
///
/// ```
/// use chewing::dictionary::Phrase;
///
/// let phrase = Phrase::new("測", 1);
/// assert_eq!(phrase, ("測", 1).into());
/// assert_eq!(("測".to_string(), 1u32), phrase.into());
/// ```
///
/// Phrases are ordered by their frequency.
///
/// ```
/// use chewing::dictionary::Phrase;
///
/// assert!(Phrase::new("測", 100) > Phrase::new("冊", 1));
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Phrase {
    phrase: String,
    freq: u32,
    last_used: Option<u64>,
}

impl Phrase {
    /// Creates a new `Phrase`.
    ///
    /// # Examples
    ///
    /// ```
    /// use chewing::dictionary::Phrase;
    ///
    /// let phrase = Phrase::new("新", 1);
    /// ```
    pub fn new<S>(phrase: S, freq: u32) -> Phrase
    where
        S: Into<String>,
    {
        Phrase {
            phrase: phrase.into(),
            freq,
            last_used: None,
        }
    }
    /// Sets the last used time of the phrase.
    pub fn with_time(mut self, last_used: u64) -> Phrase {
        self.last_used = Some(last_used);
        self
    }
    /// Returns the frequency of the phrase.
    ///
    /// # Examples
    ///
    /// ```
    /// use chewing::dictionary::Phrase;
    ///
    /// let phrase = Phrase::new("詞頻", 100);
    ///
    /// assert_eq!(100, phrase.freq());
    /// ```
    pub fn freq(&self) -> u32 {
        self.freq
    }
    /// Returns the last time this phrase was selected as user phrase.
    ///
    /// The time is a counter increased by one for each keystroke.
    pub fn last_used(&self) -> Option<u64> {
        self.last_used
    }
    /// Returns the inner str of the phrase.
    ///
    /// # Examples
    ///
    /// ```
    /// use chewing::dictionary::Phrase;
    ///
    /// let phrase = Phrase::new("詞", 100);
    ///
    /// assert_eq!("詞", phrase.as_str());
    /// ```
    pub fn as_str(&self) -> &str {
        self.phrase.borrow()
    }
}

/// Phrases are compared by their frequency first, followed by their phrase
/// string.
impl PartialOrd for Phrase {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.freq.partial_cmp(&other.freq) {
            Some(Ordering::Equal) => {}
            ord => return ord,
        }
        self.phrase.partial_cmp(&other.phrase)
    }
}

/// Phrases are compared by their frequency first, followed by their phrase
/// string.
impl Ord for Phrase {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl AsRef<str> for Phrase {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<Phrase> for String {
    fn from(phrase: Phrase) -> Self {
        phrase.phrase
    }
}

impl From<Phrase> for (String, u32) {
    fn from(phrase: Phrase) -> Self {
        (phrase.phrase, phrase.freq)
    }
}

impl<S> From<(S, u32)> for Phrase
where
    S: Into<String>,
{
    fn from(tuple: (S, u32)) -> Self {
        Phrase::new(tuple.0, tuple.1)
    }
}

impl<S> From<(S, u32, u64)> for Phrase
where
    S: Into<String>,
{
    fn from(tuple: (S, u32, u64)) -> Self {
        Phrase::new(tuple.0, tuple.1).with_time(tuple.2)
    }
}

impl Display for Phrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A generic iterator over the phrases and their frequency in a dictionary.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
///
/// use chewing::{dictionary::Dictionary, syl, zhuyin::Bopomofo};
///
/// let dict = HashMap::from([
///     (vec![syl![Bopomofo::C, Bopomofo::E, Bopomofo::TONE4]], vec![("測", 100).into()]),
/// ]);
///
/// for phrase in dict.lookup_all_phrases(
///     &[syl![Bopomofo::C, Bopomofo::E, Bopomofo::TONE4]]
/// ) {
///     assert_eq!("測", phrase.as_str());
///     assert_eq!(100, phrase.freq());
/// }
/// ```
pub type Phrases<'a> = Box<dyn Iterator<Item = Phrase> + 'a>;

/// TODO: doc
pub type DictEntries = Box<dyn Iterator<Item = (Vec<Syllable>, Phrase)>>;

/// An interface for looking up dictionaries.
///
/// This is the main dictionary trait. For more about the concept of
/// dictionaries generally, please see the [module-level
/// documentation][crate::dictionary].
///
/// # Examples
///
/// The std [`HashMap`] implements the `Dictionary` trait so it can be used in
/// tests.
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use std::collections::HashMap;
///
/// use chewing::{dictionary::Dictionary, syl, zhuyin::Bopomofo};
///
/// let mut dict = HashMap::new();
/// dict.add_phrase(&[syl![Bopomofo::C, Bopomofo::E, Bopomofo::TONE4]], ("測", 100).into())?;
///
/// for phrase in dict.lookup_all_phrases(
///     &[syl![Bopomofo::C, Bopomofo::E, Bopomofo::TONE4]]
/// ) {
///     assert_eq!("測", phrase.as_str());
///     assert_eq!(100, phrase.freq());
/// }
/// # Ok(())
/// # }
/// ```
pub trait Dictionary: Any + Debug {
    /// Returns first N phrases matched by the syllables.
    ///
    /// The result should use a stable order each time for the same input.
    fn lookup_first_n_phrases(&self, syllables: &dyn SyllableSlice, first: usize) -> Vec<Phrase>;
    /// Returns the first phrase matched by the syllables.
    ///
    /// The result should use a stable order each time for the same input.
    fn lookup_first_phrase(&self, syllables: &dyn SyllableSlice) -> Option<Phrase> {
        self.lookup_first_n_phrases(syllables, 1).into_iter().next()
    }
    /// Returns all phrases matched by the syllables.
    ///
    /// The result should use a stable order each time for the same input.
    fn lookup_all_phrases(&self, syllables: &dyn SyllableSlice) -> Vec<Phrase> {
        self.lookup_first_n_phrases(syllables, usize::MAX)
    }
    /// Returns an iterator to all phrases in the dictionary.
    ///
    /// Some dictionary backend does not support this operation.
    fn entries(&self) -> Option<DictEntries>;
    /// Returns information about the dictionary instance.
    fn about(&self) -> DictionaryInfo;
    /// Reopens the dictionary if it was changed by a different process
    ///
    /// It should not fail if the dictionary is read-only or able to sync across
    /// processes automatically.
    fn reopen(&mut self) -> Result<(), DictionaryUpdateError>;
    /// Flushes all the changes back to the filesystem
    ///
    /// The change made to the dictionary might not be persisted without
    /// calling this method.
    fn flush(&mut self) -> Result<(), DictionaryUpdateError>;
    /// An method for updating dictionaries.
    ///
    /// For more about the concept of dictionaries generally, please see the
    /// [module-level documentation][crate::dictionary].
    ///
    /// # Examples
    ///
    /// The std [`HashMap`] implements the `DictionaryMut` trait so it can be used in
    /// tests.
    ///
    /// ```
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::collections::HashMap;
    ///
    /// use chewing::{dictionary::Dictionary, syl, zhuyin::Bopomofo};
    ///
    /// let mut dict = HashMap::new();
    /// dict.add_phrase(&[syl![Bopomofo::C, Bopomofo::E, Bopomofo::TONE4]], ("測", 100).into())?;
    /// # Ok(())
    /// # }
    /// ```
    /// TODO: doc
    fn add_phrase(
        &mut self,
        syllables: &dyn SyllableSlice,
        phrase: Phrase,
    ) -> Result<(), DictionaryUpdateError>;

    /// TODO: doc
    fn update_phrase(
        &mut self,
        syllables: &dyn SyllableSlice,
        phrase: Phrase,
        user_freq: u32,
        time: u64,
    ) -> Result<(), DictionaryUpdateError>;

    /// TODO: doc
    fn remove_phrase(
        &mut self,
        syllables: &dyn SyllableSlice,
        phrase_str: &str,
    ) -> Result<(), DictionaryUpdateError>;
}

/// TODO: doc
#[derive(Error, Debug)]
#[error("build dictionary error")]
pub struct BuildDictionaryError {
    #[from]
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl From<std::io::Error> for BuildDictionaryError {
    fn from(source: std::io::Error) -> Self {
        BuildDictionaryError {
            source: Box::new(source),
        }
    }
}

/// TODO: doc
pub trait DictionaryBuilder {
    /// TODO: doc
    fn set_info(&mut self, info: DictionaryInfo) -> Result<(), BuildDictionaryError>;
    /// TODO: doc
    fn insert(
        &mut self,
        syllables: &[Syllable],
        phrase: Phrase,
    ) -> Result<(), BuildDictionaryError>;
    /// TODO: doc
    fn build(&mut self, path: &Path) -> Result<(), BuildDictionaryError>;
}

impl Dictionary for HashMap<Vec<Syllable>, Vec<Phrase>> {
    fn lookup_first_n_phrases(&self, syllables: &dyn SyllableSlice, first: usize) -> Vec<Phrase> {
        let syllables = dbg!(syllables.as_slice().into_owned());
        let mut phrases = dbg!(self.get(&syllables).cloned().unwrap_or_default());
        phrases.truncate(first);
        dbg!(phrases)
    }

    fn entries(&self) -> Option<DictEntries> {
        Some(Box::new(self.clone().into_iter().flat_map(|(k, v)| {
            v.into_iter().map(move |phrase| (k.clone(), phrase.clone()))
        })))
    }

    fn about(&self) -> DictionaryInfo {
        Default::default()
    }

    fn reopen(&mut self) -> Result<(), DictionaryUpdateError> {
        Ok(())
    }

    fn flush(&mut self) -> Result<(), DictionaryUpdateError> {
        Ok(())
    }

    fn add_phrase(
        &mut self,
        syllables: &dyn SyllableSlice,
        phrase: Phrase,
    ) -> Result<(), DictionaryUpdateError> {
        let syllables = syllables.as_slice().into_owned();
        let vec = self.entry(syllables).or_default();
        if vec.iter().any(|it| it.as_str() == phrase.as_str()) {
            return Err(DictionaryUpdateError {
                source: Some(Box::new(DuplicatePhraseError)),
            });
        }
        vec.push(phrase);
        Ok(())
    }

    fn update_phrase(
        &mut self,
        _syllables: &dyn SyllableSlice,
        _phrase: Phrase,
        _user_freq: u32,
        _time: u64,
    ) -> Result<(), DictionaryUpdateError> {
        Ok(())
    }

    fn remove_phrase(
        &mut self,
        syllables: &dyn SyllableSlice,
        phrase_str: &str,
    ) -> Result<(), DictionaryUpdateError> {
        let syllables = syllables.as_slice().into_owned();
        let vec = self.entry(syllables).or_default();
        *vec = vec
            .iter()
            .cloned()
            .filter(|p| p.as_str() != phrase_str)
            .collect::<Vec<_>>();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{dictionary::Phrase, syl, zhuyin::Bopomofo::*};

    use super::Dictionary;

    #[test]
    fn hashmap_lookup_first_one() {
        let dict = HashMap::from([(
            vec![syl![C, E, TONE4], syl![SH, TONE4]],
            vec![("測試", 1).into(), ("策試", 1).into(), ("策士", 1).into()],
        )]);

        assert_eq!(
            "測試",
            dict.lookup_first_phrase(&[syl![C, E, TONE4], syl![SH, TONE4]])
                .unwrap()
                .as_str()
        )
    }

    #[test]
    fn hashmap_lookup_all() {
        let dict = HashMap::from([(
            vec![syl![C, E, TONE4], syl![SH, TONE4]],
            vec![("測試", 1).into(), ("策試", 1).into(), ("策士", 1).into()],
        )]);

        assert_eq!(
            vec![
                Phrase::new("測試", 1),
                Phrase::new("策試", 1),
                Phrase::new("策士", 1)
            ],
            dict.lookup_all_phrases(&[syl![C, E, TONE4], syl![SH, TONE4]])
        )
    }
}