//! Decks: what a player brings to a game (CR 100.2), plus their sideboard
//! (CR 100.4) and, in Commander, their commander (CR 903.3).
//!
//! A deck refers to cards by printing ([`CardRef`]) instead of copying them,
//! so the saved file stays short and readable. Resolve it against a
//! [`CardPool`](crate::pool::CardPool) to get the cards themselves, e.g. to
//! [validate](Deck::validate) it.

mod store;
mod validate;

pub use store::{DeckStore, TomlDeckStore};
pub use validate::Violation;

use serde::{Deserialize, Serialize};

use crate::card::Card;

/// The deck construction rules a deck is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// At least 60 cards, at most four of each card other than basic lands,
    /// and a sideboard of at most 15 (CR 100.2a, 100.4a).
    Constructed,
    /// At least 40 cards built from a sealed or draft pool; the rest of the
    /// pool is the sideboard (CR 100.2b, 100.4b).
    Limited,
    /// Exactly 100 cards including the commander, one of each card other than
    /// basic lands, within the commander's color identity, and no sideboard
    /// (CR 903.5).
    Commander,
}

/// A specific printing of a card: its set and collector number. The name is
/// kept for readability and for the per-name rules (CR 100.2a, 903.5b).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardRef {
    pub name: String,
    pub set: String,
    pub collector_number: String,
}

impl From<&Card> for CardRef {
    fn from(card: &Card) -> Self {
        CardRef {
            name: card.name.clone(),
            set: card.set.clone(),
            collector_number: card.collector_number.clone(),
        }
    }
}

/// `count` copies of one printing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub count: u32,
    #[serde(flatten)]
    pub card: CardRef,
}

/// A deck under construction or ready to play.
///
/// Building methods never reject a card: a deck is usually illegal while it's
/// being built. Call [`validate`](Deck::validate) to check it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deck {
    pub name: String,
    pub format: Format,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Not part of [`main`](Deck::main), but counts toward the 100 cards
    /// (CR 903.5a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commander: Option<CardRef>,
    #[serde(default)]
    pub main: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sideboard: Vec<Entry>,
}

impl Deck {
    pub fn new(name: impl Into<String>, format: Format) -> Self {
        Deck {
            name: name.into(),
            format,
            description: String::new(),
            commander: None,
            main: Vec::new(),
            sideboard: Vec::new(),
        }
    }

    /// Adds `count` copies of `card` to the main deck.
    pub fn add(&mut self, card: &Card, count: u32) {
        add(&mut self.main, card, count);
    }

    /// Removes up to `count` copies of `card` from the main deck and returns
    /// how many were removed.
    pub fn remove(&mut self, card: &Card, count: u32) -> u32 {
        remove(&mut self.main, card, count)
    }

    /// Adds `count` copies of `card` to the sideboard.
    pub fn add_to_sideboard(&mut self, card: &Card, count: u32) {
        add(&mut self.sideboard, card, count);
    }

    /// Removes up to `count` copies of `card` from the sideboard and returns
    /// how many were removed.
    pub fn remove_from_sideboard(&mut self, card: &Card, count: u32) -> u32 {
        remove(&mut self.sideboard, card, count)
    }

    pub fn set_commander(&mut self, card: &Card) {
        self.commander = Some(card.into());
    }

    /// Cards in the main deck, not counting the commander.
    pub fn main_count(&self) -> u32 {
        self.main.iter().map(|e| e.count).sum()
    }

    pub fn sideboard_count(&self) -> u32 {
        self.sideboard.iter().map(|e| e.count).sum()
    }
}

fn add(entries: &mut Vec<Entry>, card: &Card, count: u32) {
    if count == 0 {
        return;
    }
    let card = CardRef::from(card);
    match entries.iter_mut().find(|e| e.card == card) {
        Some(entry) => entry.count += count,
        None => entries.push(Entry { count, card }),
    }
}

fn remove(entries: &mut Vec<Entry>, card: &Card, count: u32) -> u32 {
    let card = CardRef::from(card);
    let Some(i) = entries.iter().position(|e| e.card == card) else {
        return 0;
    };
    let removed = count.min(entries[i].count);
    entries[i].count -= removed;
    if entries[i].count == 0 {
        entries.remove(i);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::testing::card;

    #[test]
    fn adding_a_printing_twice_merges_entries() {
        let bolt = card("Bolt", "1", "{R}", "Instant", "");
        let mut deck = Deck::new("Burn", Format::Constructed);
        deck.add(&bolt, 3);
        deck.add(&bolt, 1);
        assert_eq!(deck.main.len(), 1);
        assert_eq!(deck.main_count(), 4);
    }

    #[test]
    fn removing_drops_empty_entries() {
        let bolt = card("Bolt", "1", "{R}", "Instant", "");
        let mut deck = Deck::new("Burn", Format::Constructed);
        deck.add(&bolt, 2);
        assert_eq!(deck.remove(&bolt, 5), 2);
        assert!(deck.main.is_empty());
        assert_eq!(deck.remove(&bolt, 1), 0);
    }

    #[test]
    fn sideboard_is_separate_from_main() {
        let bolt = card("Bolt", "1", "{R}", "Instant", "");
        let mut deck = Deck::new("Burn", Format::Constructed);
        deck.add(&bolt, 2);
        deck.add_to_sideboard(&bolt, 1);
        assert_eq!((deck.main_count(), deck.sideboard_count()), (2, 1));
    }
}
