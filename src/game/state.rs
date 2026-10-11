//! The pieces of game state: players, objects, zones and the stack.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::effect::Effect;
use super::mana::ManaPool;
use super::script::CardScript;
use crate::card::Card;

/// A player's seat: 0 or 1 in a two-player game.
pub type PlayerId = usize;

/// Identifies an object (CR 109.1): a card or token in a zone, or a spell or
/// ability on the stack. An object that moves to another zone becomes a new
/// object with a new id (CR 400.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(pub u32);

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// The game zones (CR 400.1), minus the command zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Library,
    Hand,
    Graveyard,
    Battlefield,
    Stack,
    Exile,
}

/// What a spell or ability targets (CR 115.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Player(PlayerId),
    Object(ObjectId),
}

/// A card or token in a zone (CR 109.1).
#[derive(Debug, Clone)]
pub struct GameObject {
    pub id: ObjectId,
    pub card: Arc<Card>,
    pub script: Option<Arc<CardScript>>,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub zone: Zone,
    pub token: bool,
    /// Permanent status (CR 110.5).
    pub tapped: bool,
    /// Damage marked on the permanent (CR 120.6).
    pub damage: u32,
    /// Dealt damage by a source with deathtouch since state-based actions
    /// were last checked (CR 704.5h).
    pub deathtouch_damage: bool,
    /// +1/+1 counters (CR 122.1a).
    pub plus_counters: u32,
    /// Hasn't been under its controller's control continuously since their
    /// most recent turn began (CR 302.6).
    pub summoning_sick: bool,
}

impl GameObject {
    pub fn name(&self) -> &str {
        &self.card.name
    }
}

/// A player and their zones (CR 102, 400.2).
#[derive(Debug, Clone)]
pub struct Player {
    pub life: i32,
    pub poison: u32,
    /// Top card last.
    pub library: Vec<ObjectId>,
    pub hand: Vec<ObjectId>,
    /// Top card last.
    pub graveyard: Vec<ObjectId>,
    pub mana: ManaPool,
    pub lands_played: u32,
    /// Tried to draw from an empty library since state-based actions were
    /// last checked (CR 704.5b).
    pub drew_from_empty: bool,
    pub lost: bool,
}

impl Player {
    pub(crate) fn new() -> Self {
        Player {
            // CR 103.4: each player begins the game with 20 life.
            life: 20,
            poison: 0,
            library: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            mana: ManaPool::default(),
            lands_played: 0,
            drew_from_empty: false,
            lost: false,
        }
    }
}

/// A spell or ability on the stack (CR 405.1).
#[derive(Debug, Clone)]
pub struct StackItem {
    pub id: ObjectId,
    pub controller: PlayerId,
    pub kind: StackKind,
    /// The chosen mode of a modal spell or ability (CR 700.2).
    pub mode: Option<usize>,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone)]
pub enum StackKind {
    /// A spell; the card object on the stack has the same id.
    Spell,
    /// An activated or triggered ability of `source` (CR 113.7).
    Ability { source: ObjectId, effect: Effect },
}

/// How a game ended (CR 104.2, 104.4a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameResult {
    Winner(PlayerId),
    Draw,
}

/// A continuous effect that lasts until end of turn (CR 514.2), e.g.
/// "target creature gets +2/+2 until end of turn".
#[derive(Debug, Clone)]
pub(crate) struct TempEffect {
    pub object: ObjectId,
    pub power: i32,
    pub toughness: i32,
    pub keywords: Vec<crate::card::Keyword>,
}
