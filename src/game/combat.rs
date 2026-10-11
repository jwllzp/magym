//! The combat phase: declaring attackers and blockers, and combat damage
//! (CR 506–511).

use std::collections::VecDeque;

use super::action::{BlockOption, DecisionKind};
use super::event::Event;
use super::ops::opponent;
use super::state::{ObjectId, PlayerId, Target};
use super::turn::Step;
use super::{Game, IllegalAction, Stage};
use crate::card::{CardType, Keyword};

type Result<T> = std::result::Result<T, IllegalAction>;

fn illegal<T>(why: impl Into<String>) -> Result<T> {
    Err(IllegalAction(why.into()))
}

/// The state of the current combat phase.
#[derive(Debug, Clone, Default)]
pub(crate) struct Combat {
    pub attackers: Vec<ObjectId>,
    /// (blocker, attacker) pairs.
    pub blocks: Vec<(ObjectId, ObjectId)>,
    /// Attackers that became blocked (CR 509.1h).
    pub blocked: Vec<ObjectId>,
    /// Creatures that had first strike or double strike as the first-strike
    /// damage step began, if there was one (CR 510.4).
    pub first_strikers: Option<Vec<ObjectId>>,
    /// Creatures that assign combat damage in this damage step.
    pub dealers: Vec<ObjectId>,
    /// Attackers whose damage still has to be assigned in this step.
    pub queue: VecDeque<ObjectId>,
    /// Combat damage assigned so far in this step: (source, recipient, amount).
    pub assigned: Vec<(ObjectId, Target, u32)>,
}

impl Game {
    /// Attacking creatures still in combat (CR 506.4).
    pub(crate) fn attackers(&self) -> Vec<ObjectId> {
        let Some(combat) = &self.combat else {
            return Vec::new();
        };
        combat
            .attackers
            .iter()
            .copied()
            .filter(|id| self.in_combat(*id))
            .collect()
    }

    /// Blocking creatures still in combat, with the attacker each blocks.
    pub(crate) fn blocks(&self) -> Vec<(ObjectId, ObjectId)> {
        let Some(combat) = &self.combat else {
            return Vec::new();
        };
        combat
            .blocks
            .iter()
            .copied()
            .filter(|(b, _)| self.in_combat(*b))
            .collect()
    }

    /// Attacking and blocking creatures.
    pub(crate) fn combatants(&self) -> Vec<ObjectId> {
        let mut ids = self.attackers();
        ids.extend(self.blocks().into_iter().map(|(b, _)| b));
        ids
    }

    /// A permanent leaves combat when it leaves the battlefield or stops
    /// being a creature (CR 506.4).
    fn in_combat(&self, id: ObjectId) -> bool {
        self.battlefield.contains(&id) && self.is_type(id, CardType::Creature)
    }

    /// CR 508.1a, 302.6, 702.3b.
    pub(crate) fn attack_candidates(&self) -> Vec<ObjectId> {
        self.battlefield
            .iter()
            .copied()
            .filter(|&id| {
                let obj = &self.objects[&id];
                obj.controller == self.active
                    && obj.card.is(CardType::Creature)
                    && !obj.tapped
                    && (!obj.summoning_sick || self.has_keyword(id, &Keyword::Haste))
                    && !self.has_keyword(id, &Keyword::Defender)
            })
            .collect()
    }

    pub(crate) fn declare_attackers(&mut self, attackers: Vec<ObjectId>) -> Result<()> {
        let candidates = self.attack_candidates();
        for (i, id) in attackers.iter().enumerate() {
            if !candidates.contains(id) {
                return illegal(format!("{id} can't attack"));
            }
            if attackers[..i].contains(id) {
                return illegal(format!("{id} is declared twice"));
            }
        }
        for &id in &attackers {
            // CR 508.1f, 702.20b.
            if !self.has_keyword(id, &Keyword::Vigilance) {
                self.objects.get_mut(&id).unwrap().tapped = true;
            }
            self.emit(Event::AttackerDeclared { attacker: id });
        }
        self.combat.get_or_insert_default().attackers = attackers;
        self.priority_to_active();
        Ok(())
    }

    /// Whether `blocker` may block `attacker` given evasion abilities
    /// (CR 509.1b).
    fn can_block(&self, blocker: ObjectId, attacker: ObjectId) -> bool {
        // CR 702.9b.
        if self.has_keyword(attacker, &Keyword::Flying)
            && !self.has_keyword(blocker, &Keyword::Flying)
            && !self.has_keyword(blocker, &Keyword::Reach)
        {
            return false;
        }
        // CR 702.13b.
        if self.has_keyword(attacker, &Keyword::Intimidate) {
            let b = &self.objects[&blocker].card;
            let a = &self.objects[&attacker].card;
            if !b.is(CardType::Artifact) && !b.colors.iter().any(|c| a.colors.contains(c)) {
                return false;
            }
        }
        true
    }

    /// CR 509.1a: the defending player's untapped creatures, with the
    /// attackers each may block.
    pub(crate) fn block_candidates(&self) -> Vec<BlockOption> {
        let defender = opponent(self.active);
        let attackers = self.attackers();
        self.battlefield
            .iter()
            .copied()
            .filter(|&id| {
                let obj = &self.objects[&id];
                obj.controller == defender && obj.card.is(CardType::Creature) && !obj.tapped
            })
            .filter_map(|blocker| {
                let can: Vec<ObjectId> = attackers
                    .iter()
                    .copied()
                    .filter(|a| self.can_block(blocker, *a))
                    .collect();
                (!can.is_empty()).then_some(BlockOption {
                    blocker,
                    attackers: can,
                })
            })
            .collect()
    }

    pub(crate) fn declare_blockers(&mut self, blocks: Vec<(ObjectId, ObjectId)>) -> Result<()> {
        let candidates = self.block_candidates();
        for (i, (blocker, attacker)) in blocks.iter().enumerate() {
            let Some(option) = candidates.iter().find(|o| o.blocker == *blocker) else {
                return illegal(format!("{blocker} can't block"));
            };
            if !option.attackers.contains(attacker) {
                return illegal(format!("{blocker} can't block {attacker}"));
            }
            if blocks[..i].iter().any(|(b, _)| b == blocker) {
                return illegal(format!("{blocker} can block only one creature"));
            }
        }
        // CR 702.111b.
        for attacker in self.attackers() {
            let n = blocks.iter().filter(|(_, a)| *a == attacker).count();
            if n == 1 && self.has_keyword(attacker, &Keyword::Menace) {
                return illegal(format!(
                    "{attacker} has menace and can't be blocked except by two or more creatures"
                ));
            }
        }
        for &(blocker, attacker) in &blocks {
            self.emit(Event::BlockerDeclared { blocker, attacker });
        }
        let combat = self.combat.get_or_insert_default();
        combat.blocked = blocks.iter().map(|(_, a)| *a).collect();
        combat.blocked.sort();
        combat.blocked.dedup();
        combat.blocks = blocks;
        self.priority_to_active();
        Ok(())
    }

    /// Works out which creatures deal damage in this step (CR 510.4).
    pub(crate) fn begin_damage_step(&mut self) {
        let combatants = self.combatants();
        let strikes_first = |game: &Game, id: ObjectId| {
            game.has_keyword(id, &Keyword::FirstStrike)
                || game.has_keyword(id, &Keyword::DoubleStrike)
        };
        let dealers: Vec<ObjectId> = if self.step == Step::FirstStrikeDamage {
            let first: Vec<ObjectId> = combatants
                .iter()
                .copied()
                .filter(|id| strikes_first(self, *id))
                .collect();
            self.combat.as_mut().unwrap().first_strikers = Some(first.clone());
            first
        } else {
            match &self.combat.as_ref().unwrap().first_strikers {
                None => combatants,
                Some(first) => combatants
                    .iter()
                    .copied()
                    .filter(|id| {
                        !first.contains(id) || self.has_keyword(*id, &Keyword::DoubleStrike)
                    })
                    .collect(),
            }
        };
        let queue = self
            .attackers()
            .into_iter()
            .filter(|a| dealers.contains(a))
            .collect();
        let combat = self.combat.as_mut().unwrap();
        combat.dealers = dealers;
        combat.queue = queue;
        combat.assigned.clear();
        self.stage = Stage::CombatDamage;
    }

    /// The creatures blocking `attacker`.
    fn blockers_of(&self, attacker: ObjectId) -> Vec<ObjectId> {
        self.blocks()
            .into_iter()
            .filter(|(_, a)| *a == attacker)
            .map(|(b, _)| b)
            .collect()
    }

    /// Damage that would be lethal to `blocker` from `attacker`, counting
    /// damage already marked and assigned this step (CR 702.19b, 702.2c).
    fn lethal_damage(&self, attacker: ObjectId, blocker: ObjectId) -> u32 {
        let assigned: u32 = self
            .combat
            .as_ref()
            .unwrap()
            .assigned
            .iter()
            .filter(|(_, t, _)| *t == Target::Object(blocker))
            .map(|(_, _, n)| n)
            .sum();
        let left = self.toughness(blocker) - self.objects[&blocker].damage as i32 - assigned as i32;
        let left = left.max(0) as u32;
        if self.has_keyword(attacker, &Keyword::Deathtouch) {
            left.min(1)
        } else {
            left
        }
    }

    /// CR 510.1: assigns combat damage, asking attacking players to divide
    /// it where they have a choice, then deals it all at once (CR 510.2).
    pub(crate) fn combat_damage(&mut self) {
        let defender = opponent(self.active);
        while let Some(attacker) = self.combat.as_ref().unwrap().queue.front().copied() {
            let power = self.power(attacker).max(0) as u32;
            let blockers = self.blockers_of(attacker);
            let trample = self.has_keyword(attacker, &Keyword::Trample);
            let blocked = self.combat.as_ref().unwrap().blocked.contains(&attacker);
            let assignment: Vec<(Target, u32)> = if power == 0 || !self.in_combat(attacker) {
                // CR 510.1a.
                vec![]
            } else if !blocked || blockers.is_empty() && trample {
                // CR 510.1b, 702.19d.
                vec![(Target::Player(defender), power)]
            } else if blockers.is_empty() {
                // CR 510.1c: blocked, but nothing is blocking it now.
                vec![]
            } else if blockers.len() == 1
                && (!trample || power <= self.lethal_damage(attacker, blockers[0]))
            {
                vec![(Target::Object(blockers[0]), power)]
            } else {
                let blockers = blockers
                    .iter()
                    .map(|b| (*b, self.lethal_damage(attacker, *b)))
                    .collect();
                self.pending = Some(super::action::Decision {
                    player: self.active,
                    kind: DecisionKind::AssignCombatDamage {
                        attacker,
                        power,
                        blockers,
                        trample_to: trample.then_some(defender),
                    },
                });
                return;
            };
            self.record_assignment(attacker, assignment);
        }

        // CR 510.1d: each blocking creature assigns its damage to the
        // creature it blocks.
        let dealers = self.combat.as_ref().unwrap().dealers.clone();
        for (blocker, attacker) in self.blocks() {
            let power = self.power(blocker).max(0) as u32;
            if dealers.contains(&blocker) && power > 0 && self.in_combat(attacker) {
                self.combat.as_mut().unwrap().assigned.push((
                    blocker,
                    Target::Object(attacker),
                    power,
                ));
            }
        }

        // CR 510.2.
        let assigned = std::mem::take(&mut self.combat.as_mut().unwrap().assigned);
        let sources: Vec<_> = assigned
            .iter()
            .map(|(source, _, _)| self.damage_source(*source, self.objects[source].controller))
            .collect();
        for (source, (_, target, amount)) in sources.iter().zip(assigned) {
            self.deal_damage(source, target, amount, true);
        }
        self.priority_to_active();
    }

    fn record_assignment(&mut self, attacker: ObjectId, assignment: Vec<(Target, u32)>) {
        let combat = self.combat.as_mut().unwrap();
        combat.queue.pop_front();
        combat
            .assigned
            .extend(assignment.into_iter().map(|(t, n)| (attacker, t, n)));
    }

    /// Checks and records a player's division of combat damage.
    pub(crate) fn assign_combat_damage(
        &mut self,
        attacker: ObjectId,
        power: u32,
        blockers: &[(ObjectId, u32)],
        trample_to: Option<PlayerId>,
        assignments: Vec<(Target, u32)>,
    ) -> Result<()> {
        let total: u32 = assignments.iter().map(|(_, n)| n).sum();
        if total != power {
            return illegal(format!("assign exactly {power} damage"));
        }
        for (target, _) in &assignments {
            let ok = match target {
                Target::Object(id) => blockers.iter().any(|(b, _)| b == id),
                Target::Player(p) => trample_to == Some(*p),
            };
            if !ok {
                return illegal(
                    "damage can go only to blocking creatures, or the player with trample",
                );
            }
        }
        let to = |id: ObjectId| -> u32 {
            assignments
                .iter()
                .filter(|(t, _)| *t == Target::Object(id))
                .map(|(_, n)| n)
                .sum()
        };
        // CR 702.19b: excess damage goes to the player only once every
        // blocker is assigned lethal damage.
        let to_player = assignments
            .iter()
            .any(|(t, n)| matches!(t, Target::Player(_)) && *n > 0);
        if to_player && blockers.iter().any(|(b, lethal)| to(*b) < *lethal) {
            return illegal("assign lethal damage to every blocker before the player");
        }
        let assignments = assignments.into_iter().filter(|(_, n)| *n > 0).collect();
        self.record_assignment(attacker, assignments);
        Ok(())
    }
}
