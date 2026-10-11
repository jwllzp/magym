//! Game events, and the triggered abilities that watch for them (CR 603).

use serde::{Deserialize, Serialize};

use super::Game;
use super::effect::EachSpec;
use super::mana::Mana;
use super::state::{GameResult, ObjectId, PlayerId, Target, Zone};
use super::turn::Step;
use crate::card::CardType;

/// Something that happened in the game. The game keeps a log of every event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    TurnBegan {
        turn: u32,
        player: PlayerId,
    },
    StepBegan {
        step: Step,
    },
    /// An object moved from one zone to another and became the new object
    /// `new` (CR 400.7). `from` is `None` for a token being created.
    ZoneChanged {
        old: Option<ObjectId>,
        new: ObjectId,
        name: String,
        from: Option<Zone>,
        to: Zone,
    },
    CardDrawn {
        player: PlayerId,
        card: ObjectId,
    },
    LandPlayed {
        player: PlayerId,
        card: ObjectId,
    },
    SpellCast {
        player: PlayerId,
        spell: ObjectId,
    },
    AbilityActivated {
        player: PlayerId,
        source: ObjectId,
        ability: ObjectId,
    },
    AbilityTriggered {
        player: PlayerId,
        source: ObjectId,
        ability: ObjectId,
    },
    ManaAdded {
        player: PlayerId,
        mana: Vec<Mana>,
    },
    Resolved {
        id: ObjectId,
    },
    Countered {
        id: ObjectId,
    },
    /// Removed from the stack because all its targets became illegal
    /// (CR 608.2b).
    Fizzled {
        id: ObjectId,
    },
    DamageDealt {
        source: ObjectId,
        target: Target,
        amount: u32,
        combat: bool,
    },
    LifeGained {
        player: PlayerId,
        amount: u32,
    },
    LifeLost {
        player: PlayerId,
        amount: u32,
    },
    AttackerDeclared {
        attacker: ObjectId,
    },
    BlockerDeclared {
        blocker: ObjectId,
        attacker: ObjectId,
    },
    PlayerLost {
        player: PlayerId,
        reason: LossReason,
    },
    GameOver {
        result: GameResult,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LossReason {
    /// CR 704.5a.
    ZeroLife,
    /// CR 704.5b.
    DrewFromEmptyLibrary,
    /// CR 704.5c.
    Poison,
    /// CR 104.3a.
    Conceded,
}

/// The trigger condition of a triggered ability (CR 603.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventPattern {
    /// "When/Whenever [object] enters" (CR 603.6a).
    Enters { object: ObjectRef },
    /// "When/Whenever [creature] dies" (CR 700.4).
    Dies { object: ObjectRef },
    /// "Whenever [creature] attacks" (CR 508.3a).
    Attacks { object: ObjectRef },
    /// "Whenever [creature] blocks" (CR 509.3a).
    Blocks { object: ObjectRef },
    /// "Whenever [creature] deals combat damage to a player" (CR 510.3a).
    DealsCombatDamageToPlayer { object: ObjectRef },
    /// "At the beginning of [whose] [step]" (CR 500.6).
    BeginningOfStep { step: Step, player: Whose },
    /// "When you cast this spell" (`object = "self"`) or "Whenever [player]
    /// casts a [kind] spell" (CR 601.2i).
    Cast {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        object: Option<ObjectRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        player: Option<Whose>,
    },
}

/// `"self"`, or a description such as `{ each = "creature", controller = "you" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ObjectRef {
    This(ThisRef),
    Matching(EachSpec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThisRef {
    #[serde(rename = "self")]
    This,
}

/// Whose turn or action, relative to the ability's controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Whose {
    You,
    Opponent,
    Each,
}

impl Whose {
    fn matches(self, player: PlayerId, controller: PlayerId) -> bool {
        match self {
            Whose::You => player == controller,
            Whose::Opponent => player != controller,
            Whose::Each => true,
        }
    }
}

/// A triggered ability waiting to be put on the stack (CR 603.2, 603.3).
#[derive(Debug, Clone)]
pub(crate) struct PendingTrigger {
    pub source: ObjectId,
    /// CR 603.3a: the controller of its source when it triggered.
    pub controller: PlayerId,
    /// Index into the source's `triggered` abilities.
    pub ability: usize,
}

impl Game {
    pub(crate) fn emit(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Checks the events since the last check against every triggered
    /// ability, and queues the ones that trigger (CR 603.2).
    pub(crate) fn check_triggers(&mut self) {
        let events = self.events[self.checked..].to_vec();
        self.checked = self.events.len();
        if events.is_empty() {
            return;
        }
        // CR 603.10a: leaves-the-battlefield abilities look back in time, so
        // permanents that left in this batch of events can still trigger.
        let left: Vec<ObjectId> = events
            .iter()
            .filter_map(|e| match e {
                Event::ZoneChanged {
                    old: Some(old),
                    from: Some(Zone::Battlefield),
                    ..
                } => Some(*old),
                _ => None,
            })
            .collect();
        let present = self.battlefield.clone();

        for event in &events {
            for (&source, looks_back) in present
                .iter()
                .map(|s| (s, false))
                .chain(left.iter().map(|s| (s, true)))
            {
                let Some(obj) = self.obj_or_lki(source) else {
                    continue;
                };
                let Some(script) = obj.script.clone() else {
                    continue;
                };
                let controller = obj.controller;
                for (i, ability) in script.triggered.iter().enumerate() {
                    let leaves = matches!(ability.on, EventPattern::Dies { .. });
                    if (leaves || !looks_back)
                        && self.trigger_matches(&ability.on, event, source, controller)
                    {
                        self.triggers.push(PendingTrigger {
                            source,
                            controller,
                            ability: i,
                        });
                    }
                }
            }
            // "When you cast this spell" triggers from the stack.
            if let Event::SpellCast { spell, .. } = event
                && let Some(obj) = self.objects.get(spell)
                && let Some(script) = obj.script.clone()
            {
                for (i, ability) in script.triggered.iter().enumerate() {
                    if let EventPattern::Cast {
                        object: Some(ObjectRef::This(_)),
                        ..
                    } = ability.on
                    {
                        self.triggers.push(PendingTrigger {
                            source: *spell,
                            controller: obj.controller,
                            ability: i,
                        });
                    }
                }
            }
        }
    }

    fn trigger_matches(
        &self,
        pattern: &EventPattern,
        event: &Event,
        source: ObjectId,
        controller: PlayerId,
    ) -> bool {
        match (pattern, event) {
            (
                EventPattern::Enters { object },
                Event::ZoneChanged {
                    new,
                    to: Zone::Battlefield,
                    ..
                },
            ) => self.object_ref_matches(object, *new, source, controller),
            (
                EventPattern::Dies { object },
                Event::ZoneChanged {
                    old: Some(old),
                    from: Some(Zone::Battlefield),
                    to: Zone::Graveyard,
                    ..
                },
            ) => {
                self.obj_or_lki(*old)
                    .is_some_and(|o| o.card.is(CardType::Creature))
                    && self.object_ref_matches(object, *old, source, controller)
            }
            (EventPattern::Attacks { object }, Event::AttackerDeclared { attacker }) => {
                self.object_ref_matches(object, *attacker, source, controller)
            }
            (EventPattern::Blocks { object }, Event::BlockerDeclared { blocker, .. }) => {
                self.object_ref_matches(object, *blocker, source, controller)
            }
            (
                EventPattern::DealsCombatDamageToPlayer { object },
                Event::DamageDealt {
                    source: dealer,
                    target: Target::Player(_),
                    combat: true,
                    ..
                },
            ) => self.object_ref_matches(object, *dealer, source, controller),
            (EventPattern::BeginningOfStep { step, player }, Event::StepBegan { step: began }) => {
                step == began && player.matches(self.active, controller)
            }
            (
                EventPattern::Cast { object, player },
                Event::SpellCast {
                    player: caster,
                    spell,
                },
            ) => {
                player.is_none_or(|w| w.matches(*caster, controller))
                    && match object {
                        None => true,
                        // Handled from the stack in `check_triggers`.
                        Some(ObjectRef::This(_)) => false,
                        Some(ObjectRef::Matching(spec)) => {
                            self.object_matches(spec.each, &spec.filter, *spell, controller, source)
                        }
                    }
            }
            _ => false,
        }
    }

    fn object_ref_matches(
        &self,
        object: &ObjectRef,
        id: ObjectId,
        source: ObjectId,
        controller: PlayerId,
    ) -> bool {
        match object {
            ObjectRef::This(_) => id == source,
            ObjectRef::Matching(spec) => self.object_matches_anywhere(spec, id, controller, source),
        }
    }
}
