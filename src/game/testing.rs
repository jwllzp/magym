//! Helpers for engine tests: test cards, scripts, and a builder that sets up
//! a game at any point of a turn.

use super::action::{Action, DecisionKind};
use super::script::{CardScript, ScriptPool};
use super::state::{ObjectId, PlayerId, Zone};
use super::turn::Step;
use super::{Game, Stage};
use crate::card::testing::card;
use crate::card::{Card, Stat};

pub fn creature(name: &str, cost: &str, power: i32, toughness: i32, text: &str) -> Card {
    let mut c = card(name, "1", cost, "Creature — Test", text);
    c.power = Some(Stat::Fixed(power));
    c.toughness = Some(Stat::Fixed(toughness));
    c
}

pub fn legendary_creature(name: &str, cost: &str, power: i32, toughness: i32) -> Card {
    let mut c = card(name, "1", cost, "Legendary Creature — Test", "");
    c.power = Some(Stat::Fixed(power));
    c.toughness = Some(Stat::Fixed(toughness));
    c
}

/// A basic land, e.g. `basic("Forest")`.
pub fn basic(land_type: &str) -> Card {
    card(land_type, "1", "", &format!("Basic Land — {land_type}"), "")
}

pub fn instant(name: &str, cost: &str) -> Card {
    card(name, "1", cost, "Instant", "")
}

pub fn sorcery(name: &str, cost: &str) -> Card {
    card(name, "1", cost, "Sorcery", "")
}

pub fn script(toml_text: &str) -> CardScript {
    toml::from_str(toml_text).unwrap()
}

/// Sets up a game with cards placed straight into zones, starting at a step
/// of player 0's turn.
pub struct GameBuilder {
    pub game: Game,
    scripts: ScriptPool,
}

impl GameBuilder {
    pub fn new(scripts: Vec<CardScript>) -> Self {
        GameBuilder {
            game: Game::empty(0),
            scripts: ScriptPool::new(scripts),
        }
    }

    pub fn add(&mut self, owner: PlayerId, zone: Zone, card: Card) -> ObjectId {
        let id = self.game.add_card(owner, card, &self.scripts, zone);
        if zone == Zone::Battlefield {
            // Permanents start the test under their controller's control
            // since before the turn began.
            self.game.objects.get_mut(&id).unwrap().summoning_sick = false;
        }
        id
    }

    pub fn hand(&mut self, owner: PlayerId, card: Card) -> ObjectId {
        self.add(owner, Zone::Hand, card)
    }

    pub fn battlefield(&mut self, owner: PlayerId, card: Card) -> ObjectId {
        self.add(owner, Zone::Battlefield, card)
    }

    pub fn library(&mut self, owner: PlayerId, card: Card) -> ObjectId {
        self.add(owner, Zone::Library, card)
    }

    /// Gives both players `n` cards in their libraries, so nobody decks out.
    pub fn libraries(&mut self, n: usize) {
        for p in 0..2 {
            for _ in 0..n {
                self.library(p, basic("Wastes Filler"));
            }
        }
    }

    /// Starts player 0's turn 2 at `step`; the step's turn-based actions
    /// happen first.
    pub fn start(mut self, step: Step) -> Game {
        let game = &mut self.game;
        game.turn = 2;
        game.active = 0;
        game.step = step;
        game.stage = Stage::StepBegin;
        game.run();
        self.game
    }
}

/// Answers the pending decision, panicking if the action is illegal.
pub fn act(game: &mut Game, action: Action) {
    let player = game.pending().expect("no decision pending").player;
    if let Err(e) = game.apply(player, action.clone()) {
        panic!("{action:?} by player {player}: {e}");
    }
}

/// Passes priority.
pub fn pass(game: &mut Game) {
    act(game, Action::Pass);
}

/// Passes priority until both players pass with the stack empty in a step
/// before `step`, i.e. until the game reaches `step` (or a non-priority
/// decision).
pub fn pass_until(game: &mut Game, step: Step) {
    for _ in 0..200 {
        match game.pending() {
            Some(d) if d.kind == DecisionKind::Priority && game.step() != step => pass(game),
            _ => return,
        }
    }
    panic!("didn't reach {step:?}");
}

/// Passes priority until the stack is empty.
pub fn resolve_stack(game: &mut Game) {
    for _ in 0..200 {
        if game.stack().is_empty() {
            return;
        }
        match game.pending() {
            Some(d) if d.kind == DecisionKind::Priority => pass(game),
            _ => return,
        }
    }
    panic!("stack didn't resolve");
}

/// Passes, attacking and blocking with nothing, until the next turn begins.
pub fn finish_turn(game: &mut Game) {
    let turn = game.turn();
    for _ in 0..200 {
        if game.turn() != turn || game.pending().is_none() {
            return;
        }
        let action = match decision(game) {
            DecisionKind::Priority => Action::Pass,
            DecisionKind::DeclareAttackers { .. } => Action::DeclareAttackers { attackers: vec![] },
            DecisionKind::DeclareBlockers { .. } => Action::DeclareBlockers { blocks: vec![] },
            other => panic!("unexpected decision {other:?}"),
        };
        act(game, action);
    }
    panic!("the turn didn't end");
}

pub fn decision(game: &Game) -> &DecisionKind {
    &game.pending().expect("no decision pending").kind
}
