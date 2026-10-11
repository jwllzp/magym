//! Turn structure: phases and steps, and the turn-based actions that happen
//! in them (CR 500–514, 703).

use serde::{Deserialize, Serialize};

use super::action::{Decision, DecisionKind};
use super::combat::Combat;
use super::event::Event;
use super::ops::opponent;
use super::state::Zone;
use super::{Game, Stage};
use crate::card::Keyword;

/// The steps of a turn, in order (CR 500.1). Main phases have no steps and
/// are listed as one step each. A turn has a first-strike combat damage step
/// only if a creature in combat has first strike or double strike (CR 510.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginningOfCombat,
    DeclareAttackers,
    DeclareBlockers,
    FirstStrikeDamage,
    CombatDamage,
    EndOfCombat,
    PostcombatMain,
    End,
    Cleanup,
}

/// CR 514.1: the maximum hand size is normally seven.
pub const MAX_HAND_SIZE: usize = 7;

impl Game {
    /// Starts the current step: its turn-based actions, then priority.
    pub(crate) fn begin_step(&mut self) {
        let step = self.step;
        self.emit(Event::StepBegan { step });
        match step {
            Step::Untap => {
                // CR 502.3: the active player untaps their permanents. No
                // player gets priority (CR 502.4).
                for id in self.battlefield.clone() {
                    let obj = self.objects.get_mut(&id).unwrap();
                    if obj.controller == self.active {
                        obj.tapped = false;
                    }
                }
                self.stage = Stage::StepEnd;
            }
            Step::Draw => {
                // CR 504.1.
                self.draw(self.active);
                self.priority_to_active();
            }
            Step::DeclareAttackers => {
                // CR 508.1.
                self.combat = Some(Combat::default());
                let candidates = self.attack_candidates();
                if candidates.is_empty() {
                    self.priority_to_active();
                } else {
                    self.ask(self.active, DecisionKind::DeclareAttackers { candidates });
                }
            }
            Step::DeclareBlockers => {
                // CR 509.1.
                let candidates = self.block_candidates();
                if candidates.is_empty() {
                    self.priority_to_active();
                } else {
                    self.ask(
                        opponent(self.active),
                        DecisionKind::DeclareBlockers { candidates },
                    );
                }
            }
            Step::FirstStrikeDamage | Step::CombatDamage => self.begin_damage_step(),
            Step::Cleanup => {
                // CR 514.1.
                let excess = self.players[self.active]
                    .hand
                    .len()
                    .saturating_sub(MAX_HAND_SIZE);
                if excess > 0 {
                    self.ask(self.active, DecisionKind::Discard { count: excess });
                } else {
                    self.finish_cleanup();
                }
            }
            Step::Upkeep
            | Step::PrecombatMain
            | Step::BeginningOfCombat
            | Step::EndOfCombat
            | Step::PostcombatMain
            | Step::End => self.priority_to_active(),
        }
    }

    /// Asks `player` for a turn-based decision and waits.
    pub(crate) fn ask(&mut self, player: usize, kind: DecisionKind) {
        self.pending = Some(Decision { player, kind });
        self.stage = Stage::Waiting;
    }

    /// The active player receives priority (CR 117.3a, 117.3b), once
    /// state-based actions and triggers are dealt with.
    pub(crate) fn priority_to_active(&mut self) {
        self.priority = self.active;
        self.passes = 0;
        self.stage = Stage::Settle;
    }

    /// CR 514.2, 514.3.
    pub(crate) fn finish_cleanup(&mut self) {
        for id in self.battlefield.clone() {
            self.objects.get_mut(&id).unwrap().damage = 0;
        }
        self.temp.clear();
        self.check_triggers();
        // CR 514.3a: if state-based actions or triggers need handling, the
        // active player gets priority and another cleanup step follows.
        if self.sba_pending() || !self.triggers.is_empty() {
            self.cleanup_again = true;
            self.priority_to_active();
        } else {
            self.stage = Stage::StepEnd;
        }
    }

    /// Ends the current step and moves to the next one (CR 500.2, 500.3).
    pub(crate) fn end_step(&mut self) {
        // CR 500.5: mana empties from mana pools as each step and phase ends.
        for player in &mut self.players {
            player.mana.clear();
        }
        let next = match self.step {
            Step::Untap => Step::Upkeep,
            // CR 103.8a: the starting player skips the draw step of their
            // first turn.
            Step::Upkeep if self.turn == 1 => Step::PrecombatMain,
            Step::Upkeep => Step::Draw,
            Step::Draw => Step::PrecombatMain,
            Step::PrecombatMain => Step::BeginningOfCombat,
            Step::BeginningOfCombat => Step::DeclareAttackers,
            // CR 508.8.
            Step::DeclareAttackers if self.attackers().is_empty() => Step::EndOfCombat,
            Step::DeclareAttackers => Step::DeclareBlockers,
            // CR 510.4.
            Step::DeclareBlockers if self.first_strike_in_combat() => Step::FirstStrikeDamage,
            Step::DeclareBlockers | Step::FirstStrikeDamage => Step::CombatDamage,
            Step::CombatDamage => Step::EndOfCombat,
            Step::EndOfCombat => {
                // CR 511.3.
                self.combat = None;
                Step::PostcombatMain
            }
            Step::PostcombatMain => Step::End,
            Step::End => Step::Cleanup,
            Step::Cleanup if self.cleanup_again => {
                self.cleanup_again = false;
                Step::Cleanup
            }
            Step::Cleanup => {
                self.begin_turn(opponent(self.active));
                return;
            }
        };
        self.step = next;
        self.stage = Stage::StepBegin;
    }

    /// Starts `player`'s turn.
    pub(crate) fn begin_turn(&mut self, player: usize) {
        self.turn += 1;
        self.active = player;
        self.step = Step::Untap;
        self.stage = Stage::StepBegin;
        self.players[player].lands_played = 0;
        // CR 302.6: the player's permanents have now been under their
        // control continuously since the turn began.
        for id in self.battlefield.clone() {
            let obj = self.objects.get_mut(&id).unwrap();
            if obj.controller == player {
                obj.summoning_sick = false;
            }
        }
        self.emit(Event::TurnBegan {
            turn: self.turn,
            player,
        });
    }

    /// CR 510.4: whether an attacking or blocking creature has first strike
    /// or double strike.
    fn first_strike_in_combat(&self) -> bool {
        self.combatants().into_iter().any(|id| {
            self.has_keyword(id, &Keyword::FirstStrike)
                || self.has_keyword(id, &Keyword::DoubleStrike)
        })
    }

    /// Discards `cards` from `player`'s hand to the graveyard.
    pub(crate) fn discard(&mut self, player: usize, cards: &[super::state::ObjectId]) {
        for &card in cards {
            if self.players[player].hand.contains(&card) {
                self.move_object(card, Zone::Graveyard);
            }
        }
    }
}
