//! Priority, putting triggered abilities on the stack, and resolving the top
//! of the stack (CR 117, 405, 603.3, 608).

use std::collections::VecDeque;

use super::action::{Decision, DecisionKind, TargetOptions, TriggerInfo};
use super::effect::{Ctx, Effect, Work};
use super::event::{Event, PendingTrigger};
use super::ops::opponent;
use super::sba::SbaOutcome;
use super::state::{ObjectId, PlayerId, StackItem, StackKind, Target, Zone};
use super::{Game, IllegalAction, Stage};
use crate::card::CardType;

type Result<T> = std::result::Result<T, IllegalAction>;

fn illegal<T>(why: impl Into<String>) -> Result<T> {
    Err(IllegalAction(why.into()))
}

/// A spell or ability in the middle of resolving.
#[derive(Debug, Clone)]
pub(crate) struct Resolution {
    pub id: ObjectId,
    pub ctx: Ctx,
    pub work: VecDeque<Work>,
    /// Each target, or `None` if it became illegal (CR 608.2b).
    pub targets: Vec<Option<Target>>,
}

impl Game {
    /// CR 117.5: before a player gets priority, state-based actions are
    /// performed and triggered abilities are put on the stack, until neither
    /// happens.
    pub(crate) fn settle(&mut self) {
        loop {
            match self.state_based_actions() {
                SbaOutcome::Waiting => return,
                SbaOutcome::Performed => {
                    self.check_triggers();
                    if self.result.is_some() {
                        return;
                    }
                    continue;
                }
                SbaOutcome::None => {}
            }
            self.check_triggers();
            if self.triggers.is_empty() && self.trigger_queue.is_empty() {
                break;
            }
            if !self.place_triggers() {
                return;
            }
        }
        self.pending = Some(Decision {
            player: self.priority,
            kind: DecisionKind::Priority,
        });
        self.stage = Stage::Priority;
    }

    /// CR 117.3d, 117.4.
    pub(crate) fn pass(&mut self, player: PlayerId) {
        self.passes += 1;
        if self.passes < self.players.len() {
            self.priority = opponent(player);
            self.stage = Stage::Settle;
            return;
        }
        self.passes = 0;
        if self.stack.is_empty() {
            // CR 500.2.
            self.stage = Stage::StepEnd;
        } else {
            // CR 405.5.
            self.start_resolution();
        }
    }

    /// After taking an action, a player keeps priority (CR 117.3c).
    pub(crate) fn acted(&mut self, player: PlayerId) {
        self.passes = 0;
        self.priority = player;
        self.stage = Stage::Settle;
    }

    /// The effect of a triggered ability.
    fn trigger_effect(&self, trigger: &PendingTrigger) -> Effect {
        let script = self
            .obj_or_lki(trigger.source)
            .unwrap()
            .script
            .as_ref()
            .unwrap();
        script.triggered[trigger.ability].effect.clone()
    }

    /// The modes that can be chosen for `effect` and their legal targets
    /// (CR 603.3c, 603.3d). Modes with no legal choice for a target are left
    /// out.
    pub(crate) fn target_options(
        &self,
        effect: &Effect,
        controller: PlayerId,
        source: ObjectId,
    ) -> Vec<TargetOptions> {
        let modes: Vec<Option<usize>> = match effect.modes() {
            Some(n) => (0..n).map(Some).collect(),
            None => vec![None],
        };
        modes
            .into_iter()
            .map(|mode| TargetOptions {
                mode,
                targets: effect
                    .targets(mode)
                    .iter()
                    .map(|spec| self.legal_targets(spec, controller, source))
                    .collect(),
            })
            .filter(|o| o.targets.iter().all(|t| !t.is_empty()))
            .collect()
    }

    /// Puts waiting triggered abilities on the stack in APNAP order, each
    /// player ordering their own (CR 603.3b). Returns false while waiting
    /// for a player's choice.
    fn place_triggers(&mut self) -> bool {
        loop {
            if let Some(trigger) = self.trigger_queue.front().cloned() {
                let effect = self.trigger_effect(&trigger);
                let options = self.target_options(&effect, trigger.controller, trigger.source);
                let single = options.len() == 1 && options[0].targets.iter().all(|t| t.len() == 1);
                if options.is_empty() {
                    // CR 603.3c, 603.3d: no legal choice; it's removed.
                    self.trigger_queue.pop_front();
                } else if single {
                    let mode = options[0].mode;
                    let targets = options[0].targets.iter().map(|t| t[0]).collect();
                    self.trigger_queue.pop_front();
                    self.put_trigger(&trigger, effect, mode, targets);
                } else {
                    self.pending = Some(Decision {
                        player: trigger.controller,
                        kind: DecisionKind::ChooseTargets {
                            source: trigger.source,
                            options,
                        },
                    });
                    return false;
                }
                continue;
            }
            let Some(player) = self
                .apnap()
                .into_iter()
                .find(|p| self.triggers.iter().any(|t| t.controller == *p))
            else {
                return true;
            };
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.triggers)
                .into_iter()
                .partition(|t| t.controller == player);
            self.triggers = rest;
            if mine.len() > 1 {
                let triggers = mine
                    .iter()
                    .map(|t| TriggerInfo {
                        source: t.source,
                        name: self.obj_or_lki(t.source).unwrap().name().to_string(),
                        ability: t.ability,
                    })
                    .collect();
                self.trigger_order = mine;
                self.pending = Some(Decision {
                    player,
                    kind: DecisionKind::OrderTriggers { triggers },
                });
                return false;
            }
            self.trigger_queue.extend(mine);
        }
    }

    fn put_trigger(
        &mut self,
        trigger: &PendingTrigger,
        effect: Effect,
        mode: Option<usize>,
        targets: Vec<Target>,
    ) {
        let id = self.alloc_id();
        self.stack.push(StackItem {
            id,
            controller: trigger.controller,
            kind: StackKind::Ability {
                source: trigger.source,
                effect,
            },
            mode,
            targets,
        });
        self.emit(Event::AbilityTriggered {
            player: trigger.controller,
            source: trigger.source,
            ability: id,
        });
    }

    pub(crate) fn order_triggers(&mut self, order: Vec<usize>) -> Result<()> {
        let n = self.trigger_order.len();
        let mut sorted = order.clone();
        sorted.sort();
        if sorted != (0..n).collect::<Vec<_>>() {
            return illegal(format!("give an order of all {n} triggers"));
        }
        let pending = std::mem::take(&mut self.trigger_order);
        self.trigger_queue
            .extend(order.into_iter().map(|i| pending[i].clone()));
        Ok(())
    }

    pub(crate) fn choose_trigger_targets(
        &mut self,
        mode: Option<usize>,
        targets: Vec<Target>,
    ) -> Result<()> {
        let trigger = self.trigger_queue.front().cloned().unwrap();
        let effect = self.trigger_effect(&trigger);
        if !effect.valid_mode(mode) {
            return illegal("invalid mode");
        }
        let specs = effect.targets(mode);
        if specs.len() != targets.len() {
            return illegal(format!("expected {} targets", specs.len()));
        }
        for (spec, target) in specs.iter().zip(&targets) {
            if !self.target_legal(spec, *target, trigger.controller, trigger.source) {
                return illegal("illegal target");
            }
        }
        self.trigger_queue.pop_front();
        self.put_trigger(&trigger, effect, mode, targets);
        Ok(())
    }

    /// Begins resolving the top object of the stack (CR 608.2).
    fn start_resolution(&mut self) {
        let item = self.stack.last().unwrap().clone();
        let (effect, ctx) = match &item.kind {
            StackKind::Spell => (
                self.objects[&item.id]
                    .script
                    .as_ref()
                    .and_then(|s| s.spell.clone()),
                Ctx {
                    controller: item.controller,
                    source: item.id,
                },
            ),
            StackKind::Ability { source, effect } => (
                Some(effect.clone()),
                Ctx {
                    controller: item.controller,
                    source: *source,
                },
            ),
        };
        let (instrs, specs) = match &effect {
            Some(e) => e.plan(item.mode),
            None => (Vec::new(), Vec::new()),
        };
        // CR 608.2b.
        let targets: Vec<Option<Target>> = specs
            .iter()
            .zip(&item.targets)
            .map(|(spec, t)| {
                self.target_legal(spec, *t, item.controller, ctx.source)
                    .then_some(*t)
            })
            .collect();
        if !targets.is_empty() && targets.iter().all(Option::is_none) {
            self.emit(Event::Fizzled { id: item.id });
            match item.kind {
                StackKind::Spell => {
                    self.move_object(item.id, Zone::Graveyard);
                }
                StackKind::Ability { .. } => self.stack.retain(|s| s.id != item.id),
            }
            self.priority_to_active();
            return;
        }
        self.resolution = Some(Resolution {
            id: item.id,
            ctx,
            work: instrs.into_iter().map(Work::Instr).collect(),
            targets,
        });
        self.stage = Stage::Resolving;
    }

    /// Carries out the next step of the resolving spell or ability.
    pub(crate) fn continue_resolution(&mut self) {
        let res = self.resolution.as_mut().unwrap();
        let Some(work) = res.work.pop_front() else {
            self.finish_resolution();
            return;
        };
        let (ctx, targets) = (res.ctx, res.targets.clone());
        match work {
            Work::Instr(instr) => {
                let follow_up = self.execute(&instr, ctx, &targets);
                let res = self.resolution.as_mut().unwrap();
                for w in follow_up.into_iter().rev() {
                    res.work.push_front(w);
                }
            }
            Work::Discard { player, count } => {
                let hand = self.players[player].hand.clone();
                if hand.len() <= count as usize {
                    self.discard(player, &hand);
                } else {
                    self.resolution
                        .as_mut()
                        .unwrap()
                        .work
                        .push_front(Work::Discard { player, count });
                    self.pending = Some(Decision {
                        player,
                        kind: DecisionKind::Discard {
                            count: count as usize,
                        },
                    });
                }
            }
        }
    }

    /// The player whose discard choice the resolving effect is waiting for.
    pub(crate) fn resolution_discard(
        &mut self,
        player: PlayerId,
        cards: &[ObjectId],
    ) -> Result<()> {
        let res = self.resolution.as_mut().unwrap();
        let Some(Work::Discard { count, .. }) = res.work.front() else {
            return illegal("not waiting for a discard");
        };
        let count = *count as usize;
        self.check_discard(player, cards, count)?;
        self.resolution.as_mut().unwrap().work.pop_front();
        self.discard(player, cards);
        Ok(())
    }

    /// Checks that `cards` are `count` different cards in `player`'s hand.
    pub(crate) fn check_discard(
        &self,
        player: PlayerId,
        cards: &[ObjectId],
        count: usize,
    ) -> Result<()> {
        if cards.len() != count {
            return illegal(format!("choose exactly {count} cards"));
        }
        for (i, card) in cards.iter().enumerate() {
            if !self.players[player].hand.contains(card) || cards[..i].contains(card) {
                return illegal(format!("{card} isn't a different card in your hand"));
            }
        }
        Ok(())
    }

    /// CR 608.2n, 608.3a.
    fn finish_resolution(&mut self) {
        let res = self.resolution.take().unwrap();
        let Some(item) = self.stack.iter().find(|s| s.id == res.id).cloned() else {
            // It left the stack while resolving.
            self.priority_to_active();
            return;
        };
        self.emit(Event::Resolved { id: item.id });
        match item.kind {
            StackKind::Spell => {
                let permanent = !self.is_type(item.id, CardType::Instant)
                    && !self.is_type(item.id, CardType::Sorcery);
                if permanent {
                    self.move_object_to(item.id, Zone::Battlefield, Some(item.controller), false);
                } else {
                    self.move_object(item.id, Zone::Graveyard);
                }
            }
            StackKind::Ability { .. } => self.stack.retain(|s| s.id != item.id),
        }
        self.priority_to_active();
    }
}
