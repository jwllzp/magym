//! Every downloaded card, indexed by printing.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::card::Card;
use crate::deck::CardRef;
use crate::error::Error;
use crate::store;

/// The cards a deck can be built from and resolved against.
#[derive(Debug, Clone, Default)]
pub struct CardPool {
    cards: Vec<Card>,
    by_printing: HashMap<(String, String), usize>,
}

impl CardPool {
    pub fn new(cards: Vec<Card>) -> Self {
        let by_printing = cards
            .iter()
            .enumerate()
            .map(|(i, c)| ((c.set.clone(), c.collector_number.clone()), i))
            .collect();
        CardPool { cards, by_printing }
    }

    /// Loads every set directory under `root`, e.g. `data/`.
    pub fn load(root: &Path) -> Result<Self, Error> {
        let mut dirs = Vec::new();
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_dir() {
                dirs.push(path);
            }
        }
        dirs.sort();
        let mut cards = Vec::new();
        for dir in dirs {
            cards.extend(store::read_set(&dir)?);
        }
        Ok(CardPool::new(cards))
    }

    /// The printing `card` refers to.
    pub fn get(&self, card: &CardRef) -> Option<&Card> {
        self.by_printing
            .get(&(card.set.clone(), card.collector_number.clone()))
            .map(|&i| &self.cards[i])
    }

    /// Every printing with this exact English name.
    pub fn named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Card> {
        self.cards.iter().filter(move |c| c.name == name)
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }
}
