//! The effect DSL: the building blocks card scripts are written with, and
//! how the engine carries them out as a spell or ability resolves (CR 608.2c).
//!
//! An effect names what happens (`effect = "deal_damage"`) and who or what it
//! happens to through a [`Selector`]:
//!
//! - a string for a fixed choice: `"you"`, `"each_opponent"`, `"each_player"`,
//!   or `"this"` (the spell's or ability's source);
//! - `{ kind = "creature", ... }` for a target, chosen as the spell is cast or
//!   the ability is put on the stack (CR 115.1);
//! - `{ each = "creature", ... }` for every object or player that matches.
//!
//! ```toml
//! effect = "deal_damage"
//! amount = 3
//! target = { kind = "any" }
//! ```

use serde::{Deserialize, Serialize};

use super::Game;
use super::state::{ObjectId, PlayerId, Target, TempEffect, Zone};
use crate::card::{
    Ability, Card, Color, Keyword, ManaCost, Rarity, Stat, TypeLine, parse_abilities,
};

/// Something a spell or ability does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    /// CR 120.
    DealDamage {
        amount: u32,
        target: Selector,
    },
    /// CR 119.
    GainLife {
        amount: u32,
        #[serde(default = "you")]
        player: Selector,
    },
    LoseLife {
        amount: u32,
        #[serde(default = "you")]
        player: Selector,
    },
    /// CR 121.
    Draw {
        #[serde(default = "one")]
        amount: u32,
        #[serde(default = "you")]
        player: Selector,
    },
    /// The player chooses which cards to discard (CR 701.9).
    Discard {
        #[serde(default = "one")]
        amount: u32,
        #[serde(default = "you")]
        player: Selector,
    },
    /// CR 701.17.
    Mill {
        #[serde(default = "one")]
        amount: u32,
        #[serde(default = "you")]
        player: Selector,
    },
    /// CR 701.8.
    Destroy {
        target: Selector,
    },
    /// CR 701.13.
    Exile {
        target: Selector,
    },
    /// Put into its owner's hand.
    ReturnToHand {
        target: Selector,
    },
    Tap {
        target: Selector,
    },
    Untap {
        target: Selector,
    },
    /// CR 701.6.
    CounterSpell {
        target: Selector,
    },
    /// Adds mana to the controller's pool, e.g. `mana = "{B}{B}{B}"` (CR 106.4).
    AddMana {
        mana: ManaCost,
    },
    /// "gets +N/+N and gains [keywords] until end of turn".
    Pump {
        target: Selector,
        #[serde(default)]
        power: i32,
        #[serde(default)]
        toughness: i32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keywords: Vec<Keyword>,
    },
    /// Puts +1/+1 counters (CR 122.1a).
    PutCounters {
        target: Selector,
        #[serde(default = "one")]
        amount: u32,
    },
    /// Creates tokens under the controller's control (CR 111.2).
    CreateToken {
        token: TokenSpec,
        #[serde(default = "one")]
        amount: u32,
    },
    /// Each effect in order (CR 608.2c).
    Sequence {
        effects: Vec<Effect>,
    },
    /// A modal effect: one mode is chosen as it's cast or put on the stack
    /// (CR 700.2).
    ChooseOne {
        modes: Vec<Effect>,
    },
}

fn one() -> u32 {
    1
}

fn you() -> Selector {
    Selector::Fixed(Who::You)
}

/// Who or what an effect applies to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Selector {
    Fixed(Who),
    Target(TargetSpec),
    Each(EachSpec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Who {
    /// The controller of the spell or ability.
    You,
    EachOpponent,
    EachPlayer,
    /// The spell's or ability's source, e.g. "this creature".
    This,
}

/// "target [kind]" (CR 115.1), e.g. `{ kind = "creature", controller = "opponent" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetSpec {
    pub kind: Kind,
    #[serde(flatten)]
    pub filter: Filter,
}

/// "each [kind]", e.g. `{ each = "creature", controller = "you" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EachSpec {
    pub each: Kind,
    #[serde(flatten)]
    pub filter: Filter,
}

/// The kind of object or player a selector picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A creature, player, planeswalker or battle (CR 115.4). Planeswalkers
    /// and battles aren't supported yet.
    Any,
    Player,
    Opponent,
    Creature,
    Permanent,
    NonlandPermanent,
    Artifact,
    Enchantment,
    Land,
    Spell,
    CreatureSpell,
    NoncreatureSpell,
}

impl Kind {
    pub fn includes_players(self) -> bool {
        matches!(self, Kind::Any | Kind::Player | Kind::Opponent)
    }

    pub fn includes_objects(self) -> bool {
        !matches!(self, Kind::Player | Kind::Opponent)
    }

    /// The zone the objects this kind picks are in.
    pub fn zone(self) -> Zone {
        match self {
            Kind::Spell | Kind::CreatureSpell | Kind::NoncreatureSpell => Zone::Stack,
            _ => Zone::Battlefield,
        }
    }
}

/// Extra conditions on a selected object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    /// Who controls it, relative to the spell's or ability's controller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<Relation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tapped: Option<bool>,
    /// "another" / "other": not the source itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub other: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    You,
    Opponent,
}

/// A token's characteristics (CR 111.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenSpec {
    pub name: String,
    pub type_line: TypeLine,
    #[serde(default)]
    pub colors: Vec<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<Keyword>,
}

impl TokenSpec {
    pub(crate) fn to_card(&self) -> Card {
        let text = self
            .keywords
            .iter()
            .map(|k| k.rules_name())
            .collect::<Vec<_>>()
            .join(", ");
        Card {
            name: self.name.clone(),
            mana_cost: ManaCost::default(),
            mana_value: 0,
            abilities: parse_abilities(&text, false),
            type_line: self.type_line.clone(),
            colors: self.colors.clone(),
            color_identity: self.colors.clone(),
            power: self.power.map(Stat::Fixed),
            toughness: self.toughness.map(Stat::Fixed),
            loyalty: None,
            defense: None,
            rarity: Rarity::Common,
            set: String::new(),
            collector_number: String::new(),
            id: String::new(),
            oracle_id: String::new(),
            image_url: None,
            oracle_text: text,
            back_face: None,
        }
    }
}

/// One primitive step of a resolving effect, with the target slot of each
/// of its targeted selectors in order.
#[derive(Debug, Clone)]
pub(crate) struct Instr {
    pub effect: Effect,
    pub slots: Vec<usize>,
}

impl Effect {
    /// The number of modes, if this is a modal effect.
    pub fn modes(&self) -> Option<usize> {
        match self {
            Effect::ChooseOne { modes } => Some(modes.len()),
            _ => None,
        }
    }

    /// Whether `mode` is a valid mode choice for this effect.
    pub(crate) fn valid_mode(&self, mode: Option<usize>) -> bool {
        match (self.modes(), mode) {
            (None, None) => true,
            (Some(n), Some(m)) => m < n,
            _ => false,
        }
    }

    /// The target specs of this effect with `mode` chosen, one per instance
    /// of the word "target", in order (CR 601.2c).
    pub fn targets(&self, mode: Option<usize>) -> Vec<TargetSpec> {
        self.plan(mode).1
    }

    /// The primitive steps of this effect with `mode` chosen, and its
    /// target specs.
    pub(crate) fn plan(&self, mode: Option<usize>) -> (Vec<Instr>, Vec<TargetSpec>) {
        let mut instrs = Vec::new();
        let mut specs = Vec::new();
        self.flatten(mode, &mut instrs, &mut specs);
        (instrs, specs)
    }

    fn flatten(&self, mode: Option<usize>, instrs: &mut Vec<Instr>, specs: &mut Vec<TargetSpec>) {
        match self {
            Effect::Sequence { effects } => {
                for e in effects {
                    e.flatten(mode, instrs, specs);
                }
            }
            Effect::ChooseOne { modes } => {
                if let Some(e) = mode.and_then(|m| modes.get(m)) {
                    // Modes don't nest; inner effects aren't modal.
                    e.flatten(None, instrs, specs);
                }
            }
            primitive => {
                let mut slots = Vec::new();
                for selector in primitive.selectors() {
                    if let Selector::Target(spec) = selector {
                        slots.push(specs.len());
                        specs.push(spec.clone());
                    }
                }
                instrs.push(Instr {
                    effect: primitive.clone(),
                    slots,
                });
            }
        }
    }

    /// The selectors of a primitive effect, in order.
    fn selectors(&self) -> Vec<&Selector> {
        match self {
            Effect::DealDamage { target, .. }
            | Effect::Destroy { target }
            | Effect::Exile { target }
            | Effect::ReturnToHand { target }
            | Effect::Tap { target }
            | Effect::Untap { target }
            | Effect::CounterSpell { target }
            | Effect::Pump { target, .. }
            | Effect::PutCounters { target, .. } => vec![target],
            Effect::GainLife { player, .. }
            | Effect::LoseLife { player, .. }
            | Effect::Draw { player, .. }
            | Effect::Discard { player, .. }
            | Effect::Mill { player, .. } => vec![player],
            Effect::AddMana { .. }
            | Effect::CreateToken { .. }
            | Effect::Sequence { .. }
            | Effect::ChooseOne { .. } => vec![],
        }
    }
}

/// Who controls a resolving spell or ability, and its source.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx {
    pub controller: PlayerId,
    /// The spell itself, or the source of an ability.
    pub source: ObjectId,
}

/// Work left in a resolving spell or ability.
#[derive(Debug, Clone)]
pub(crate) enum Work {
    Instr(Instr),
    /// `player` discards `count` cards of their choice.
    Discard {
        player: PlayerId,
        count: u32,
    },
}

impl Game {
    /// What `selector` refers to as the effect resolves. Targets that became
    /// illegal (`None` in `targets`) are skipped (CR 608.2b).
    fn select(
        &self,
        selector: &Selector,
        ctx: Ctx,
        slots: &mut impl Iterator<Item = usize>,
        targets: &[Option<Target>],
    ) -> Vec<Target> {
        match selector {
            Selector::Fixed(Who::You) => vec![Target::Player(ctx.controller)],
            Selector::Fixed(Who::EachOpponent) => self
                .apnap()
                .into_iter()
                .filter(|p| *p != ctx.controller)
                .map(Target::Player)
                .collect(),
            Selector::Fixed(Who::EachPlayer) => {
                self.apnap().into_iter().map(Target::Player).collect()
            }
            Selector::Fixed(Who::This) => {
                if self.objects.contains_key(&ctx.source) {
                    vec![Target::Object(ctx.source)]
                } else {
                    vec![]
                }
            }
            Selector::Target(_) => slots
                .next()
                .and_then(|slot| targets.get(slot).copied().flatten())
                .into_iter()
                .collect(),
            Selector::Each(spec) => self.each(spec, ctx.controller, ctx.source),
        }
    }

    /// Every player and object matching `spec`, players first in APNAP order.
    pub(crate) fn each(
        &self,
        spec: &EachSpec,
        controller: PlayerId,
        source: ObjectId,
    ) -> Vec<Target> {
        let mut out = Vec::new();
        if spec.each.includes_players() {
            for p in self.apnap() {
                if self.player_matches(spec.each, p, controller) {
                    out.push(Target::Player(p));
                }
            }
        }
        if spec.each.includes_objects() {
            let ids: Vec<ObjectId> = match spec.each.zone() {
                Zone::Stack => self.stack.iter().map(|s| s.id).collect(),
                _ => self.battlefield.clone(),
            };
            for id in ids {
                if self.object_matches(spec.each, &spec.filter, id, controller, source) {
                    out.push(Target::Object(id));
                }
            }
        }
        out
    }

    /// Carries out one step of a resolving effect. Returns work to do next,
    /// before the rest of the effect.
    pub(crate) fn execute(
        &mut self,
        instr: &Instr,
        ctx: Ctx,
        targets: &[Option<Target>],
    ) -> Vec<Work> {
        let mut slots = instr.slots.iter().copied();
        let mut follow_up = Vec::new();
        match &instr.effect {
            Effect::DealDamage { amount, target } => {
                let source = self.damage_source(ctx.source, ctx.controller);
                let recipients = self.select(target, ctx, &mut slots, targets);
                for recipient in recipients {
                    self.deal_damage(&source, recipient, *amount, false);
                }
            }
            Effect::GainLife { amount, player } => {
                for p in self.select_players(player, ctx, &mut slots, targets) {
                    self.gain_life(p, *amount);
                }
            }
            Effect::LoseLife { amount, player } => {
                for p in self.select_players(player, ctx, &mut slots, targets) {
                    self.lose_life(p, *amount);
                }
            }
            Effect::Draw { amount, player } => {
                for p in self.select_players(player, ctx, &mut slots, targets) {
                    for _ in 0..*amount {
                        self.draw(p);
                    }
                }
            }
            Effect::Discard { amount, player } => {
                for p in self.select_players(player, ctx, &mut slots, targets) {
                    follow_up.push(Work::Discard {
                        player: p,
                        count: *amount,
                    });
                }
            }
            Effect::Mill { amount, player } => {
                for p in self.select_players(player, ctx, &mut slots, targets) {
                    for _ in 0..*amount {
                        if let Some(&top) = self.players[p].library.last() {
                            self.move_object(top, Zone::Graveyard);
                        }
                    }
                }
            }
            Effect::Destroy { target } => {
                for id in self.select_objects(target, ctx, &mut slots, targets) {
                    self.destroy(id);
                }
            }
            Effect::Exile { target } => {
                for id in self.select_objects(target, ctx, &mut slots, targets) {
                    self.move_object(id, Zone::Exile);
                }
            }
            Effect::ReturnToHand { target } => {
                for id in self.select_objects(target, ctx, &mut slots, targets) {
                    self.move_object(id, Zone::Hand);
                }
            }
            Effect::Tap { target } => {
                for id in self.select_objects(target, ctx, &mut slots, targets) {
                    self.objects.get_mut(&id).unwrap().tapped = true;
                }
            }
            Effect::Untap { target } => {
                for id in self.select_objects(target, ctx, &mut slots, targets) {
                    self.objects.get_mut(&id).unwrap().tapped = false;
                }
            }
            Effect::CounterSpell { target } => {
                for id in self.select_objects(target, ctx, &mut slots, targets) {
                    self.counter(id);
                }
            }
            Effect::AddMana { mana } => {
                if let Some(mana) = super::mana::produced(mana) {
                    self.add_mana(ctx.controller, &mana);
                }
            }
            Effect::Pump {
                target,
                power,
                toughness,
                keywords,
            } => {
                for id in self.select_objects(target, ctx, &mut slots, targets) {
                    self.temp.push(TempEffect {
                        object: id,
                        power: *power,
                        toughness: *toughness,
                        keywords: keywords.clone(),
                    });
                }
            }
            Effect::PutCounters { target, amount } => {
                for id in self.select_objects(target, ctx, &mut slots, targets) {
                    self.objects.get_mut(&id).unwrap().plus_counters += amount;
                }
            }
            Effect::CreateToken { token, amount } => {
                for _ in 0..*amount {
                    self.create_token(token, ctx.controller);
                }
            }
            Effect::Sequence { .. } | Effect::ChooseOne { .. } => {
                unreachable!("composite effects are flattened before they resolve")
            }
        }
        follow_up
    }

    fn select_players(
        &self,
        selector: &Selector,
        ctx: Ctx,
        slots: &mut impl Iterator<Item = usize>,
        targets: &[Option<Target>],
    ) -> Vec<PlayerId> {
        self.select(selector, ctx, slots, targets)
            .into_iter()
            .filter_map(|t| match t {
                Target::Player(p) => Some(p),
                Target::Object(_) => None,
            })
            .collect()
    }

    fn select_objects(
        &self,
        selector: &Selector,
        ctx: Ctx,
        slots: &mut impl Iterator<Item = usize>,
        targets: &[Option<Target>],
    ) -> Vec<ObjectId> {
        self.select(selector, ctx, slots, targets)
            .into_iter()
            .filter_map(|t| match t {
                Target::Object(id) if self.objects.contains_key(&id) => Some(id),
                _ => None,
            })
            .collect()
    }
}

/// The keyword abilities on a printed card.
pub(crate) fn printed_keywords(card: &Card) -> impl Iterator<Item = &Keyword> {
    card.abilities.iter().filter_map(|a| match a {
        Ability::Keyword { keyword, .. } => Some(keyword),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(toml_text: &str) -> Effect {
        toml::from_str(toml_text).unwrap()
    }

    #[test]
    fn parses_targeted_damage() {
        assert_eq!(
            effect("effect = \"deal_damage\"\namount = 3\ntarget = { kind = \"any\" }"),
            Effect::DealDamage {
                amount: 3,
                target: Selector::Target(TargetSpec {
                    kind: Kind::Any,
                    filter: Filter::default()
                })
            }
        );
    }

    #[test]
    fn parses_fixed_and_each_selectors() {
        assert_eq!(
            effect("effect = \"draw\"\namount = 2"),
            Effect::Draw {
                amount: 2,
                player: Selector::Fixed(Who::You)
            }
        );
        assert_eq!(
            effect(
                "effect = \"destroy\"\ntarget = { each = \"creature\", controller = \"opponent\", tapped = true }"
            ),
            Effect::Destroy {
                target: Selector::Each(EachSpec {
                    each: Kind::Creature,
                    filter: Filter {
                        controller: Some(Relation::Opponent),
                        tapped: Some(true),
                        ..Filter::default()
                    }
                })
            }
        );
    }

    // CR 601.2c: one target per instance of the word "target", in order.
    #[test]
    fn target_slots_follow_the_text() {
        let e = effect(
            r#"
            effect = "sequence"
            [[effects]]
            effect = "deal_damage"
            amount = 2
            target = { kind = "creature" }
            [[effects]]
            effect = "draw"
            [[effects]]
            effect = "deal_damage"
            amount = 2
            target = { kind = "player" }
            "#,
        );
        let (instrs, specs) = e.plan(None);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].kind, Kind::Creature);
        assert_eq!(specs[1].kind, Kind::Player);
        let slots: Vec<Vec<usize>> = instrs.iter().map(|i| i.slots.clone()).collect();
        assert_eq!(slots, vec![vec![0], vec![], vec![1]]);
    }

    // CR 700.2: only the chosen mode's targets are chosen.
    #[test]
    fn modes_choose_their_targets() {
        let e = effect(
            r#"
            effect = "choose_one"
            [[modes]]
            effect = "destroy"
            target = { kind = "artifact" }
            [[modes]]
            effect = "gain_life"
            amount = 4
            "#,
        );
        assert_eq!(e.modes(), Some(2));
        assert_eq!(e.targets(Some(0))[0].kind, Kind::Artifact);
        assert!(e.targets(Some(1)).is_empty());
        assert!(!e.valid_mode(None));
        assert!(e.valid_mode(Some(1)));
    }
}
