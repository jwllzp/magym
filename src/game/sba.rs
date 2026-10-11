//! State-based actions (CR 704).

use super::Game;
use super::action::{Decision, DecisionKind};
use super::event::{Event, LossReason};
use super::state::{GameResult, ObjectId, PlayerId, Zone};
use crate::card::{CardType, Keyword};

pub(crate) enum SbaOutcome {
    /// No state-based actions applied.
    None,
    Performed,
    /// A player has to choose first (the legend rule).
    Waiting,
}

/// The state-based actions that apply right now.
#[derive(Default)]
struct Sba {
    losers: Vec<(PlayerId, LossReason)>,
    /// Tokens outside the battlefield (CR 704.5d).
    cease: Vec<ObjectId>,
    /// Put into the graveyard without being destroyed (CR 704.5f, 704.5j).
    graveyard: Vec<ObjectId>,
    /// Destroyed (CR 704.5g, 704.5h).
    destroy: Vec<ObjectId>,
}

impl Sba {
    fn is_empty(&self) -> bool {
        self.losers.is_empty()
            && self.cease.is_empty()
            && self.graveyard.is_empty()
            && self.destroy.is_empty()
    }
}

impl Game {
    fn find_sba(&self) -> Sba {
        let mut sba = Sba::default();
        for (p, player) in self.players.iter().enumerate() {
            if player.lost {
                continue;
            }
            if player.life <= 0 {
                sba.losers.push((p, LossReason::ZeroLife));
            } else if player.drew_from_empty {
                sba.losers.push((p, LossReason::DrewFromEmptyLibrary));
            } else if player.poison >= 10 {
                sba.losers.push((p, LossReason::Poison));
            }
        }
        sba.cease = self
            .objects
            .values()
            .filter(|o| o.token && o.zone != Zone::Battlefield)
            .map(|o| o.id)
            .collect();
        for &id in &self.battlefield {
            if !self.is_type(id, CardType::Creature) {
                continue;
            }
            let toughness = self.toughness(id);
            let obj = &self.objects[&id];
            if toughness <= 0 {
                sba.graveyard.push(id);
            } else if (obj.damage as i32 >= toughness || obj.deathtouch_damage)
                && !self.has_keyword(id, &Keyword::Indestructible)
            {
                sba.destroy.push(id);
            }
        }
        for group in self.legend_groups() {
            if let Some(keep) = group.iter().find(|id| self.legend_keep.contains(id)) {
                sba.graveyard
                    .extend(group.iter().copied().filter(|id| id != keep));
            }
        }
        sba.graveyard.sort();
        sba.graveyard.dedup();
        sba.destroy.retain(|id| !sba.graveyard.contains(id));
        sba
    }

    /// CR 704.5j: groups of legendary permanents with the same name and
    /// controller.
    fn legend_groups(&self) -> Vec<Vec<ObjectId>> {
        let mut groups: Vec<Vec<ObjectId>> = Vec::new();
        for &id in &self.battlefield {
            if !self.is_legendary(id) {
                continue;
            }
            let obj = &self.objects[&id];
            match groups.iter_mut().find(|g| {
                let first = &self.objects[&g[0]];
                first.controller == obj.controller && first.name() == obj.name()
            }) {
                Some(group) => group.push(id),
                None => groups.push(vec![id]),
            }
        }
        groups.retain(|g| g.len() > 1);
        groups
    }

    /// Whether any state-based action would be performed now.
    pub(crate) fn sba_pending(&self) -> bool {
        !self.find_sba().is_empty() || !self.legend_groups().is_empty()
    }

    /// Performs all applicable state-based actions simultaneously
    /// (CR 704.3).
    pub(crate) fn state_based_actions(&mut self) -> SbaOutcome {
        for group in self.legend_groups() {
            if !group.iter().any(|id| self.legend_keep.contains(id)) {
                let player = self.objects[&group[0]].controller;
                self.pending = Some(Decision {
                    player,
                    kind: DecisionKind::LegendRule { choices: group },
                });
                return SbaOutcome::Waiting;
            }
        }
        let sba = self.find_sba();
        self.legend_keep.clear();
        // These conditions only last until state-based actions are checked.
        for player in &mut self.players {
            player.drew_from_empty = false;
        }
        for id in self.battlefield.clone() {
            self.objects.get_mut(&id).unwrap().deathtouch_damage = false;
        }
        if sba.is_empty() {
            return SbaOutcome::None;
        }

        for &(player, reason) in &sba.losers {
            self.players[player].lost = true;
            self.emit(Event::PlayerLost { player, reason });
        }
        for id in sba.graveyard.into_iter().chain(sba.destroy) {
            self.move_object(id, Zone::Graveyard);
        }
        for id in sba.cease {
            if let Some(obj) = self.objects.remove(&id) {
                let owner = obj.owner;
                let player = &mut self.players[owner];
                player.hand.retain(|x| *x != id);
                player.library.retain(|x| *x != id);
                player.graveyard.retain(|x| *x != id);
                self.exile.retain(|x| *x != id);
                self.stack.retain(|s| s.id != id);
                self.lki.insert(id, obj);
            }
        }
        self.check_game_over();
        SbaOutcome::Performed
    }

    /// CR 104.2a, 104.4a: the game ends when a player loses; if every
    /// player loses at once, it's a draw.
    pub(crate) fn check_game_over(&mut self) {
        if self.result.is_some() {
            return;
        }
        let remaining: Vec<PlayerId> = (0..self.players.len())
            .filter(|p| !self.players[*p].lost)
            .collect();
        let result = match remaining.as_slice() {
            [] => GameResult::Draw,
            [winner] => GameResult::Winner(*winner),
            _ => return,
        };
        self.result = Some(result);
        self.pending = None;
        self.emit(Event::GameOver { result });
    }
}
