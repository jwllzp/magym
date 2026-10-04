//! Saving decks: one TOML file per deck.
//!
//! ```text
//! decks/
//!   mono-red-burn.toml
//!   atraxa-superfriends.toml
//! ```

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::Deck;
use crate::error::Error;
use crate::store::slug;

/// Where decks are saved. Decks are identified by name.
pub trait DeckStore {
    /// Saves `deck`, replacing any deck with the same name.
    fn save(&self, deck: &Deck) -> Result<(), Error>;
    /// Loads the deck called `name`.
    fn load(&self, name: &str) -> Result<Deck, Error>;
    /// Names of every saved deck, sorted.
    fn list(&self) -> Result<Vec<String>, Error>;
    /// Deletes the deck called `name`. Returns whether it existed.
    fn delete(&self, name: &str) -> Result<bool, Error>;
}

/// Saves each deck to `<dir>/<name slug>.toml`.
///
/// Names that slug the same (e.g. `Mono Red` and `mono-red`) share a file.
#[derive(Debug, Clone)]
pub struct TomlDeckStore {
    dir: PathBuf,
}

impl TomlDeckStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        TomlDeckStore { dir: dir.into() }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.toml", slug(name)))
    }

    fn read(path: &Path) -> Result<Deck, Error> {
        let text = fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|source| Error::DeckFile {
            path: path.to_path_buf(),
            source,
        })
    }
}

impl DeckStore for TomlDeckStore {
    fn save(&self, deck: &Deck) -> Result<(), Error> {
        if slug(&deck.name).is_empty() {
            return Err(Error::DeckName(deck.name.clone()));
        }
        fs::create_dir_all(&self.dir)?;
        fs::write(self.path(&deck.name), toml::to_string(deck)?)?;
        Ok(())
    }

    fn load(&self, name: &str) -> Result<Deck, Error> {
        let path = self.path(name);
        if !path.is_file() {
            return Err(Error::DeckNotFound(name.to_string()));
        }
        Self::read(&path)
    }

    fn list(&self) -> Result<Vec<String>, Error> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut names = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "toml") {
                names.push(Self::read(&path)?.name);
            }
        }
        names.sort();
        Ok(names)
    }

    fn delete(&self, name: &str) -> Result<bool, Error> {
        match fs::remove_file(self.path(name)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::testing::card;
    use crate::deck::Format;

    fn temp_store(name: &str) -> (TomlDeckStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("magym-decks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        (TomlDeckStore::new(&dir), dir)
    }

    fn sample() -> Deck {
        let mut deck = Deck::new("Mono-Red Burn", Format::Constructed);
        deck.description = "Go face.".into();
        deck.add(&card("Mountain", "1", "", "Basic Land — Mountain", ""), 20);
        deck.add(&card("Lightning Bolt", "2", "{R}", "Instant", ""), 4);
        deck.add_to_sideboard(&card("Pyroblast", "3", "{R}", "Instant", ""), 2);
        deck
    }

    #[test]
    fn toml_shape() {
        let text = toml::to_string(&sample()).unwrap();
        for line in [
            r#"name = "Mono-Red Burn""#,
            r#"format = "constructed""#,
            "[[main]]",
            "count = 4",
            r#"name = "Lightning Bolt""#,
            r#"set = "tst""#,
            r#"collector_number = "2""#,
            "[[sideboard]]",
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "missing {line:?} in:\n{text}"
            );
        }
        assert!(!text.contains("commander"), "{text}");
    }

    #[test]
    fn save_then_load_round_trips() {
        let (store, dir) = temp_store("roundtrip");
        let mut deck = sample();
        store.save(&deck).unwrap();
        assert!(dir.join("mono-red-burn.toml").is_file());
        assert_eq!(store.load("Mono-Red Burn").unwrap(), deck);

        deck.format = Format::Commander;
        deck.sideboard.clear();
        deck.set_commander(&card("Boss", "4", "{R}", "Legendary Creature — Goblin", ""));
        store.save(&deck).unwrap();
        assert_eq!(store.load("Mono-Red Burn").unwrap(), deck);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn list_and_delete() {
        let (store, dir) = temp_store("list");
        assert_eq!(store.list().unwrap(), Vec::<String>::new());
        store.save(&Deck::new("Zoo", Format::Limited)).unwrap();
        store.save(&sample()).unwrap();
        assert_eq!(store.list().unwrap(), ["Mono-Red Burn", "Zoo"]);
        assert!(store.delete("Zoo").unwrap());
        assert!(!store.delete("Zoo").unwrap());
        assert_eq!(store.list().unwrap(), ["Mono-Red Burn"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_deck_is_not_found() {
        let (store, _) = temp_store("missing");
        assert!(matches!(store.load("Nope"), Err(Error::DeckNotFound(n)) if n == "Nope"));
    }

    #[test]
    fn name_needs_a_letter_or_digit() {
        let (store, _) = temp_store("badname");
        assert!(matches!(
            store.save(&Deck::new("???", Format::Limited)),
            Err(Error::DeckName(_))
        ));
    }

    #[test]
    fn bad_file_names_its_path() {
        let (store, dir) = temp_store("bad");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("broken.toml"), "name = 3").unwrap();
        let err = store.load("broken").unwrap_err().to_string();
        assert!(err.contains("broken.toml"), "{err}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
