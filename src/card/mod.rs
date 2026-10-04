mod ability;
mod mana;
mod types;

pub use ability::{Ability, Keyword, parse_abilities};
pub use mana::{Color, ManaCost, ManaSymbol};
pub use types::{CardType, Rarity, Stat, Supertype, TypeLine};

use serde::{Deserialize, Serialize};

/// A single-faced Magic card with its printed characteristics.
///
/// Fields are in the order they are saved: printed characteristics first,
/// then ids, then the rules text and its abilities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub name: String,
    pub mana_cost: ManaCost,
    pub mana_value: u32,
    pub type_line: TypeLine,
    pub colors: Vec<Color>,
    pub color_identity: Vec<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<Stat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<Stat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty: Option<Stat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defense: Option<Stat>,
    pub rarity: Rarity,
    pub set: String,
    pub collector_number: String,
    /// Id of this printing.
    pub id: String,
    /// Id shared by every printing of the same card.
    pub oracle_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    pub oracle_text: String,
    pub abilities: Vec<Ability>,
}

impl Card {
    pub fn is(&self, card_type: CardType) -> bool {
        self.type_line.is(card_type)
    }

    pub fn has_keyword(&self, keyword: &Keyword) -> bool {
        self.abilities
            .iter()
            .any(|a| matches!(a, Ability::Keyword { keyword: k, .. } if k == keyword))
    }
}
