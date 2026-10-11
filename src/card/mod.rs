mod ability;
mod mana;
#[cfg(test)]
pub(crate) mod testing;
mod types;

pub use ability::{Ability, Keyword, parse_abilities};
pub use mana::{Color, ManaCost, ManaSymbol};
pub use types::{CardType, Rarity, Stat, Supertype, TypeLine};

use serde::{Deserialize, Serialize};

/// A Magic card with its printed characteristics.
///
/// For a double-faced card the top-level fields are its front face, which is
/// all the card has outside the game and in every zone but the battlefield and
/// the stack (CR 712.8a), and [`back_face`](Card::back_face) holds the other
/// side.
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
    /// The back face of a double-faced card (CR 712.2, 712.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_face: Option<BackFace>,
}

/// The kind of a double-faced card (CR 712.1). Meld cards aren't modeled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoubleFaced {
    /// Transforms or converts to its back face (CR 712.2), e.g. a werewolf.
    Nonmodal,
    /// Either face can be cast or played (CR 712.3).
    Modal,
}

/// The back face of a double-faced card, with its own set of characteristics
/// (CR 712.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackFace {
    pub kind: DoubleFaced,
    pub name: String,
    pub mana_cost: ManaCost,
    /// For a nonmodal card, the front face's mana value (CR 202.3b, 712.8e).
    pub mana_value: u32,
    pub type_line: TypeLine,
    /// Usually from a color indicator, since most back faces have no mana
    /// cost (CR 204).
    pub colors: Vec<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<Stat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<Stat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty: Option<Stat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defense: Option<Stat>,
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
