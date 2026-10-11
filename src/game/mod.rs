//! The game engine: plays a two-player game of Magic by the Comprehensive
//! Rules.
//!
//! The engine is a state machine that stops whenever a player has to decide
//! something. It never calls players itself: ask it what decision is
//! [pending](Game::pending), pick one of the [legal actions](Game::legal_actions),
//! and [apply](Game::apply) it. The engine then runs until the next decision.
//!
//! ```no_run
//! # use download_database::game::{Game, ScriptPool};
//! # let (decks, scripts) = ([vec![], vec![]], ScriptPool::default());
//! let mut game = Game::new(decks, &scripts, 42).unwrap();
//! while let Some(decision) = game.pending().cloned() {
//!     let action = game.legal_actions()[0].clone();
//!     game.apply(decision.player, action).unwrap();
//! }
//! println!("{:?}", game.result());
//! ```
//!
//! Card behavior beyond native keywords comes from [card scripts](script).

pub mod action;
mod cast;
mod combat;
pub mod effect;
pub mod event;
pub mod mana;
mod ops;
mod priority;
mod rng;
mod sba;
pub mod script;
pub mod state;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
mod turn;
mod view;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

pub use action::{Action, Decision, DecisionKind, ManaActivation};
pub use event::Event;
pub use mana::{Mana, ManaPool};
pub use script::{CardScript, ScriptPool};
pub use state::{GameResult, ObjectId, PlayerId, Target, Zone};
pub use turn::Step;
pub use view::{ObjectView, PlayerView, StackView, View};

use crate::card::Card;
use combat::Combat;
use event::{LossReason, PendingTrigger};
use ops::opponent;
use priority::Resolution;
use rng::Rng;
use state::{GameObject, Player, StackItem, TempEffect};

/// CR 103.5: the starting hand size is normally seven.
pub const STARTING_HAND_SIZE: usize = 7;

/// At most this many actions are listed for decisions with many possible
/// answers, such as declaring blockers.
const MAX_LISTED_ACTIONS: usize = 1024;

/// An action that isn't allowed right now. The game is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("illegal action: {0}")]
pub struct IllegalAction(pub String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GameError {
    #[error("unplayable cards: {}", .0.iter().map(|(n, why)| format!("{n} ({why})")).collect::<Vec<_>>().join(", "))]
    Unplayable(Vec<(String, String)>),
}

/// Where the engine is in its procedure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    StartingPlayer,
    Mulligan,
    /// The current step's turn-based actions are next.
    StepBegin,
    /// Waiting for a turn-based decision (attackers, blockers, discard).
    Waiting,
    CombatDamage,
    /// State-based actions and triggers, then priority (CR 117.5).
    Settle,
    Priority,
    Resolving,
    StepEnd,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Mulligans {
    pub taken: [u32; 2],
    pub kept: [bool; 2],
    /// This round's declarations: `Some(true)` to take a mulligan.
    pub declared: [Option<bool>; 2],
    /// Players who still have to put cards on the bottom.
    pub bottom: VecDeque<PlayerId>,
}

/// A game in progress.
#[derive(Debug, Clone)]
pub struct Game {
    pub(crate) objects: BTreeMap<ObjectId, GameObject>,
    /// Objects as they last existed before leaving their zone (CR 113.7a).
    pub(crate) lki: HashMap<ObjectId, GameObject>,
    pub(crate) next_id: u32,
    pub(crate) players: Vec<Player>,
    pub(crate) battlefield: Vec<ObjectId>,
    pub(crate) exile: Vec<ObjectId>,
    /// Top last.
    pub(crate) stack: Vec<StackItem>,
    pub(crate) turn: u32,
    pub(crate) active: PlayerId,
    pub(crate) step: Step,
    pub(crate) stage: Stage,
    pub(crate) priority: PlayerId,
    /// Players who passed in succession (CR 117.4).
    pub(crate) passes: usize,
    pub(crate) pending: Option<Decision>,
    pub(crate) combat: Option<Combat>,
    /// Triggered abilities waiting to be put on the stack.
    pub(crate) triggers: Vec<PendingTrigger>,
    /// A player's triggers waiting to be ordered.
    pub(crate) trigger_order: Vec<PendingTrigger>,
    /// Triggers being put on the stack, in order.
    pub(crate) trigger_queue: VecDeque<PendingTrigger>,
    pub(crate) resolution: Option<Resolution>,
    pub(crate) temp: Vec<TempEffect>,
    /// Legendary permanents chosen to stay by the legend rule.
    pub(crate) legend_keep: Vec<ObjectId>,
    pub(crate) cleanup_again: bool,
    pub(crate) mulligans: Mulligans,
    pub(crate) rng: Rng,
    pub(crate) events: Vec<Event>,
    /// Events before this index have been checked for triggers.
    pub(crate) checked: usize,
    pub(crate) result: Option<GameResult>,
}

impl Game {
    /// Starts a game between two decks, shuffled with `seed`. Fails if a deck
    /// has cards the engine can't play yet.
    pub fn new(decks: [Vec<Card>; 2], scripts: &ScriptPool, seed: u64) -> Result<Game, GameError> {
        let mut unplayable: Vec<(String, String)> = decks
            .iter()
            .flatten()
            .filter_map(|c| scripts.unplayable(c).map(|why| (c.name.clone(), why)))
            .collect();
        unplayable.sort();
        unplayable.dedup();
        if !unplayable.is_empty() {
            return Err(GameError::Unplayable(unplayable));
        }

        let mut game = Game::empty(seed);
        for (owner, deck) in decks.into_iter().enumerate() {
            for card in deck {
                game.add_card(owner, card, scripts, Zone::Library);
            }
            // CR 103.3.
            let mut library = std::mem::take(&mut game.players[owner].library);
            game.rng.shuffle(&mut library);
            game.players[owner].library = library;
        }
        // CR 103.1: a random player chooses who takes the first turn.
        let chooser = game.rng.below(2);
        game.pending = Some(Decision {
            player: chooser,
            kind: DecisionKind::ChooseStartingPlayer,
        });
        Ok(game)
    }

    pub(crate) fn empty(seed: u64) -> Game {
        Game {
            objects: BTreeMap::new(),
            lki: HashMap::new(),
            next_id: 0,
            players: vec![Player::new(), Player::new()],
            battlefield: Vec::new(),
            exile: Vec::new(),
            stack: Vec::new(),
            turn: 0,
            active: 0,
            step: Step::Untap,
            stage: Stage::StartingPlayer,
            priority: 0,
            passes: 0,
            pending: None,
            combat: None,
            triggers: Vec::new(),
            trigger_order: Vec::new(),
            trigger_queue: VecDeque::new(),
            resolution: None,
            temp: Vec::new(),
            legend_keep: Vec::new(),
            cleanup_again: false,
            mulligans: Mulligans::default(),
            rng: Rng::new(seed),
            events: Vec::new(),
            checked: 0,
            result: None,
        }
    }

    /// Puts a new card object into a zone (library, hand or graveyard of
    /// its owner, or the battlefield under its owner's control).
    pub(crate) fn add_card(
        &mut self,
        owner: PlayerId,
        card: Card,
        scripts: &ScriptPool,
        zone: Zone,
    ) -> ObjectId {
        let id = self.alloc_id();
        let script = scripts.get(&card.name).cloned();
        self.objects.insert(
            id,
            GameObject {
                id,
                card: Arc::new(card),
                script,
                owner,
                controller: owner,
                zone,
                token: false,
                tapped: false,
                damage: 0,
                deathtouch_damage: false,
                plus_counters: 0,
                summoning_sick: true,
            },
        );
        match zone {
            Zone::Library => self.players[owner].library.push(id),
            Zone::Hand => self.players[owner].hand.push(id),
            Zone::Graveyard => self.players[owner].graveyard.push(id),
            Zone::Battlefield => self.battlefield.push(id),
            Zone::Exile => self.exile.push(id),
            Zone::Stack => panic!("cards are put on the stack by casting them"),
        }
        id
    }

    /// The decision the game is waiting for, or `None` once it's over.
    pub fn pending(&self) -> Option<&Decision> {
        self.pending.as_ref()
    }

    pub fn result(&self) -> Option<GameResult> {
        self.result
    }

    /// Every event so far, oldest first.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn turn(&self) -> u32 {
        self.turn
    }

    pub fn active_player(&self) -> PlayerId {
        self.active
    }

    pub fn step(&self) -> Step {
        self.step
    }

    pub fn life(&self, player: PlayerId) -> i32 {
        self.players[player].life
    }

    pub fn hand(&self, player: PlayerId) -> &[ObjectId] {
        &self.players[player].hand
    }

    pub fn library(&self, player: PlayerId) -> &[ObjectId] {
        &self.players[player].library
    }

    pub fn graveyard(&self, player: PlayerId) -> &[ObjectId] {
        &self.players[player].graveyard
    }

    pub fn battlefield(&self) -> &[ObjectId] {
        &self.battlefield
    }

    /// The ids of the spells and abilities on the stack, top last.
    pub fn stack(&self) -> Vec<ObjectId> {
        self.stack.iter().map(|s| s.id).collect()
    }

    pub fn mana_pool(&self, player: PlayerId) -> ManaPool {
        self.players[player].mana
    }

    /// The card an object is, if it exists.
    pub fn card(&self, id: ObjectId) -> Option<&Card> {
        self.objects.get(&id).map(|o| o.card.as_ref())
    }

    pub fn zone(&self, id: ObjectId) -> Option<Zone> {
        self.objects.get(&id).map(|o| o.zone)
    }

    pub fn is_tapped(&self, id: ObjectId) -> bool {
        self.objects.get(&id).is_some_and(|o| o.tapped)
    }

    pub fn damage(&self, id: ObjectId) -> u32 {
        self.objects.get(&id).map_or(0, |o| o.damage)
    }

    pub fn controller(&self, id: ObjectId) -> Option<PlayerId> {
        self.objects.get(&id).map(|o| o.controller)
    }

    /// Every action that answers the pending decision. Decisions with very
    /// many possible answers list only the first ones.
    pub fn legal_actions(&self) -> Vec<Action> {
        let Some(decision) = &self.pending else {
            return Vec::new();
        };
        let player = decision.player;
        match &decision.kind {
            DecisionKind::ChooseStartingPlayer => (0..self.players.len())
                .map(|player| Action::ChooseStartingPlayer { player })
                .collect(),
            DecisionKind::Mulligan { .. } => vec![Action::Keep, Action::Mulligan],
            DecisionKind::PutOnBottom { count } => subsets(&self.players[player].hand, *count)
                .into_iter()
                .map(|cards| Action::PutOnBottom { cards })
                .collect(),
            DecisionKind::Priority => self.priority_actions(player),
            DecisionKind::DeclareAttackers { candidates } => {
                let mut actions = Vec::new();
                for k in 0..=candidates.len() {
                    for attackers in subsets(candidates, k) {
                        if actions.len() == MAX_LISTED_ACTIONS {
                            return actions;
                        }
                        actions.push(Action::DeclareAttackers { attackers });
                    }
                }
                actions
            }
            DecisionKind::DeclareBlockers { candidates } => {
                let mut all: Vec<Vec<(ObjectId, ObjectId)>> = vec![Vec::new()];
                for option in candidates {
                    let mut next = Vec::new();
                    for blocks in &all {
                        next.push(blocks.clone());
                        for &attacker in &option.attackers {
                            let mut b = blocks.clone();
                            b.push((option.blocker, attacker));
                            next.push(b);
                        }
                    }
                    next.truncate(MAX_LISTED_ACTIONS * 4);
                    all = next;
                }
                // Drop blocks that break restrictions such as menace.
                all.into_iter()
                    .filter(|blocks| self.clone().declare_blockers(blocks.clone()).is_ok())
                    .take(MAX_LISTED_ACTIONS)
                    .map(|blocks| Action::DeclareBlockers { blocks })
                    .collect()
            }
            DecisionKind::AssignCombatDamage {
                attacker,
                power,
                blockers,
                trample_to,
            } => {
                let mut recipients: Vec<Target> =
                    blockers.iter().map(|(b, _)| Target::Object(*b)).collect();
                recipients.extend(trample_to.map(Target::Player));
                divisions(*power, recipients.len())
                    .into_iter()
                    .map(|split| recipients.iter().copied().zip(split).collect::<Vec<_>>())
                    .filter(|assignments| {
                        self.clone()
                            .assign_combat_damage(
                                *attacker,
                                *power,
                                blockers,
                                *trample_to,
                                assignments.clone(),
                            )
                            .is_ok()
                    })
                    .take(MAX_LISTED_ACTIONS)
                    .map(|assignments| Action::AssignCombatDamage { assignments })
                    .collect()
            }
            DecisionKind::Discard { count } => subsets(&self.players[player].hand, *count)
                .into_iter()
                .take(MAX_LISTED_ACTIONS)
                .map(|cards| Action::Discard { cards })
                .collect(),
            DecisionKind::LegendRule { choices } => choices
                .iter()
                .map(|&card| Action::KeepLegend { card })
                .collect(),
            DecisionKind::OrderTriggers { triggers } => permutations(triggers.len())
                .into_iter()
                .take(MAX_LISTED_ACTIONS)
                .map(|order| Action::OrderTriggers { order })
                .collect(),
            DecisionKind::ChooseTargets { options, .. } => {
                let mut actions = Vec::new();
                for option in options {
                    let mut combos: Vec<Vec<Target>> = vec![Vec::new()];
                    for choices in &option.targets {
                        combos = combos
                            .iter()
                            .flat_map(|c| {
                                choices.iter().map(move |t| {
                                    let mut c = c.clone();
                                    c.push(*t);
                                    c
                                })
                            })
                            .take(MAX_LISTED_ACTIONS)
                            .collect();
                    }
                    actions.extend(combos.into_iter().map(|targets| Action::ChooseTargets {
                        mode: option.mode,
                        targets,
                    }));
                }
                actions.truncate(MAX_LISTED_ACTIONS);
                actions
            }
        }
    }

    /// Takes `action` for `player`, then runs the game until the next
    /// decision. An illegal action leaves the game unchanged.
    pub fn apply(&mut self, player: PlayerId, action: Action) -> Result<(), IllegalAction> {
        if self.result.is_some() {
            return Err(IllegalAction("the game is over".into()));
        }
        if player >= self.players.len() {
            return Err(IllegalAction(format!("no player {player}")));
        }
        if action == Action::Concede {
            // CR 104.3a.
            self.players[player].lost = true;
            self.emit(Event::PlayerLost {
                player,
                reason: LossReason::Conceded,
            });
            self.check_game_over();
            return Ok(());
        }
        let Some(decision) = self.pending.clone() else {
            return Err(IllegalAction("no decision is pending".into()));
        };
        if decision.player != player {
            return Err(IllegalAction(format!(
                "player {} has to decide, not player {player}",
                decision.player
            )));
        }
        self.answer(player, decision.kind, action)?;
        self.pending = None;
        self.check_triggers();
        self.run();
        Ok(())
    }

    fn answer(
        &mut self,
        player: PlayerId,
        kind: DecisionKind,
        action: Action,
    ) -> Result<(), IllegalAction> {
        let wrong = || IllegalAction(format!("{action:?} doesn't answer {kind:?}"));
        match (&kind, action.clone()) {
            (
                DecisionKind::ChooseStartingPlayer,
                Action::ChooseStartingPlayer { player: first },
            ) => {
                if first >= self.players.len() {
                    return Err(IllegalAction(format!("no player {first}")));
                }
                self.active = first;
                // CR 103.5.
                for p in [first, opponent(first)] {
                    for _ in 0..STARTING_HAND_SIZE {
                        self.draw(p);
                    }
                }
                self.stage = Stage::Mulligan;
            }
            (DecisionKind::Mulligan { .. }, Action::Keep) => {
                self.mulligans.kept[player] = true;
                self.mulligans.declared[player] = Some(false);
            }
            (DecisionKind::Mulligan { .. }, Action::Mulligan) => {
                self.mulligans.declared[player] = Some(true);
            }
            (DecisionKind::PutOnBottom { count }, Action::PutOnBottom { cards }) => {
                self.check_discard(player, &cards, *count)?;
                for card in cards {
                    self.move_object_to(card, Zone::Library, None, true);
                }
                self.mulligans.bottom.pop_front();
            }
            (DecisionKind::Priority, action) => self.priority_action(player, action)?,
            (DecisionKind::DeclareAttackers { .. }, Action::DeclareAttackers { attackers }) => {
                self.declare_attackers(attackers)?
            }
            (DecisionKind::DeclareBlockers { .. }, Action::DeclareBlockers { blocks }) => {
                self.declare_blockers(blocks)?
            }
            (
                DecisionKind::AssignCombatDamage {
                    attacker,
                    power,
                    blockers,
                    trample_to,
                },
                Action::AssignCombatDamage { assignments },
            ) => {
                self.assign_combat_damage(*attacker, *power, blockers, *trample_to, assignments)?
            }
            (DecisionKind::Discard { count }, Action::Discard { cards }) => {
                if self.stage == Stage::Resolving {
                    self.resolution_discard(player, &cards)?;
                } else {
                    self.check_discard(player, &cards, *count)?;
                    self.discard(player, &cards);
                    self.finish_cleanup();
                }
            }
            (DecisionKind::LegendRule { choices }, Action::KeepLegend { card }) => {
                if !choices.contains(&card) {
                    return Err(IllegalAction(format!("{card} isn't one of the choices")));
                }
                self.legend_keep.push(card);
            }
            (DecisionKind::OrderTriggers { .. }, Action::OrderTriggers { order }) => {
                self.order_triggers(order)?
            }
            (DecisionKind::ChooseTargets { .. }, Action::ChooseTargets { mode, targets }) => {
                self.choose_trigger_targets(mode, targets)?
            }
            _ => return Err(wrong()),
        }
        Ok(())
    }

    fn priority_action(&mut self, player: PlayerId, action: Action) -> Result<(), IllegalAction> {
        match action {
            Action::Pass => {
                self.pass(player);
                return Ok(());
            }
            Action::PlayLand { card } => self.play_land(player, card)?,
            Action::CastSpell {
                card,
                mode,
                targets,
                mana,
            } => self.cast_spell(player, card, mode, targets, mana)?,
            Action::ActivateAbility {
                source,
                ability,
                mode,
                targets,
                mana,
            } => self.activate(player, source, ability, mode, targets, mana)?,
            Action::ActivateManaAbility(activation) => self.activate_mana(player, activation)?,
            other => {
                return Err(IllegalAction(format!(
                    "{other:?} can't be taken with priority"
                )));
            }
        }
        self.acted(player);
        Ok(())
    }

    /// Runs the game until a decision is needed or it's over.
    pub(crate) fn run(&mut self) {
        while self.pending.is_none() && self.result.is_none() {
            match self.stage {
                Stage::StartingPlayer | Stage::Waiting => {
                    unreachable!("waiting for a decision that isn't pending")
                }
                Stage::Mulligan => self.mulligan_step(),
                Stage::StepBegin => self.begin_step(),
                Stage::CombatDamage => self.combat_damage(),
                Stage::Settle | Stage::Priority => self.settle(),
                Stage::Resolving => self.continue_resolution(),
                Stage::StepEnd => self.end_step(),
            }
            self.check_triggers();
        }
    }

    /// CR 103.5: the mulligan process.
    fn mulligan_step(&mut self) {
        if let Some(&player) = self.mulligans.bottom.front() {
            self.pending = Some(Decision {
                player,
                kind: DecisionKind::PutOnBottom {
                    count: self.mulligans.taken[player] as usize,
                },
            });
            return;
        }
        for player in [self.active, opponent(self.active)] {
            if self.mulligans.kept[player] || self.mulligans.declared[player].is_some() {
                continue;
            }
            // No more mulligans once the hand would be zero cards.
            if self.mulligans.taken[player] as usize >= STARTING_HAND_SIZE {
                self.mulligans.kept[player] = true;
                continue;
            }
            self.pending = Some(Decision {
                player,
                kind: DecisionKind::Mulligan {
                    hand_size: self.players[player].hand.len(),
                    mulligans: self.mulligans.taken[player],
                },
            });
            return;
        }
        let taking: Vec<PlayerId> = (0..2)
            .filter(|p| self.mulligans.declared[*p] == Some(true))
            .collect();
        self.mulligans.declared = [None; 2];
        if taking.is_empty() {
            self.begin_turn(self.active);
            return;
        }
        for player in [self.active, opponent(self.active)] {
            if !taking.contains(&player) {
                continue;
            }
            for card in self.players[player].hand.clone() {
                self.move_object(card, Zone::Library);
            }
            let mut library = std::mem::take(&mut self.players[player].library);
            self.rng.shuffle(&mut library);
            self.players[player].library = library;
            for _ in 0..STARTING_HAND_SIZE {
                self.draw(player);
            }
            self.mulligans.taken[player] += 1;
            self.mulligans.bottom.push_back(player);
        }
    }
}

/// Every `k`-element subset of `items`, in order.
fn subsets<T: Clone>(items: &[T], k: usize) -> Vec<Vec<T>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    fn go<T: Clone>(
        items: &[T],
        k: usize,
        start: usize,
        current: &mut Vec<T>,
        out: &mut Vec<Vec<T>>,
    ) {
        if out.len() >= MAX_LISTED_ACTIONS {
            return;
        }
        if current.len() == k {
            out.push(current.clone());
            return;
        }
        for i in start..items.len() {
            current.push(items[i].clone());
            go(items, k, i + 1, current, out);
            current.pop();
        }
    }
    go(items, k, 0, &mut current, &mut out);
    out
}

/// Every way to split `total` among `n` recipients.
fn divisions(total: u32, n: usize) -> Vec<Vec<u32>> {
    if n == 0 {
        return if total == 0 { vec![vec![]] } else { vec![] };
    }
    if n == 1 {
        return vec![vec![total]];
    }
    let mut out = Vec::new();
    for first in (0..=total).rev() {
        for mut rest in divisions(total - first, n - 1) {
            rest.insert(0, first);
            out.push(rest);
            if out.len() >= MAX_LISTED_ACTIONS * 4 {
                return out;
            }
        }
    }
    out
}

/// Every permutation of `0..n`.
fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![vec![]];
    }
    let mut out = Vec::new();
    for rest in permutations(n - 1) {
        for i in 0..=rest.len() {
            let mut p = rest.clone();
            p.insert(i, n - 1);
            out.push(p);
            if out.len() >= MAX_LISTED_ACTIONS {
                return out;
            }
        }
    }
    out
}
