//! Card scripts: what a card does, written as data with the effect DSL.
//!
//! Scripts live in `scripts/`, one file per card named after the card, e.g.
//! `scripts/lightning-bolt.toml`. A script covers every printing of the card.
//!
//! ```toml
//! name = "Lightning Bolt"
//!
//! [spell]
//! effect = "deal_damage"
//! amount = 3
//! target = { kind = "any" }
//! ```
//!
//! Keyword abilities the engine supports natively (see [`NATIVE_KEYWORDS`])
//! come from the card itself and aren't scripted.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::effect::{EachSpec, Effect};
use super::event::EventPattern;
use crate::card::{Ability, Card, CardType, DoubleFaced, Keyword, ManaCost, ManaSymbol, Stat};
use crate::error::Error;
use crate::store::slug;

/// The keyword abilities the engine implements itself.
pub const NATIVE_KEYWORDS: &[Keyword] = &[
    Keyword::Deathtouch,
    Keyword::Defender,
    Keyword::DoubleStrike,
    Keyword::FirstStrike,
    Keyword::Flash,
    Keyword::Flying,
    Keyword::Haste,
    Keyword::Hexproof,
    Keyword::Indestructible,
    Keyword::Intimidate,
    Keyword::Lifelink,
    Keyword::Menace,
    Keyword::Reach,
    Keyword::Shroud,
    Keyword::Trample,
    Keyword::Vigilance,
];

/// What a card does in the game, beyond its native keywords.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CardScript {
    pub name: String,
    /// What an instant or sorcery does as it resolves (CR 113.3a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spell: Option<Effect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggered: Vec<TriggeredAbility>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activated: Vec<ActivatedAbility>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mana: Vec<ManaAbility>,
    #[serde(default, rename = "static", skip_serializing_if = "Vec::is_empty")]
    pub statics: Vec<StaticAbility>,
}

/// "[When/Whenever/At] [event], [effect]" (CR 603.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggeredAbility {
    pub on: EventPattern,
    #[serde(flatten)]
    pub effect: Effect,
}

/// "[Cost]: [Effect]" (CR 602.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivatedAbility {
    pub cost: Cost,
    /// "Activate only as a sorcery" (CR 602.5d).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sorcery_speed: bool,
    #[serde(flatten)]
    pub effect: Effect,
}

/// The cost of an activated ability (CR 118, 602.2b).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    #[serde(default, skip_serializing_if = "ManaCost::is_empty")]
    pub mana: ManaCost,
    /// {T} (CR 107.5).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tap: bool,
    /// "Sacrifice this permanent".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sacrifice_self: bool,
    /// "Pay N life" (CR 119.4).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub life: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// "{T}: Add [one of `produces`]" (CR 605.1a), e.g. `produces = ["{R}", "{G}"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManaAbility {
    pub produces: Vec<ManaCost>,
}

/// A static ability that modifies other permanents (CR 604, 613.4c), e.g.
/// "Other creatures you control get +1/+1."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticAbility {
    pub affects: EachSpec,
    #[serde(default)]
    pub power: i32,
    #[serde(default)]
    pub toughness: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<Keyword>,
}

/// Every card script, by card name.
#[derive(Debug, Clone, Default)]
pub struct ScriptPool {
    by_name: HashMap<String, Arc<CardScript>>,
}

impl ScriptPool {
    pub fn new(scripts: impl IntoIterator<Item = CardScript>) -> Self {
        ScriptPool {
            by_name: scripts
                .into_iter()
                .map(|s| (s.name.clone(), Arc::new(s)))
                .collect(),
        }
    }

    /// Loads every `.toml` script in `dir`, e.g. `scripts/`.
    pub fn load(dir: &Path) -> Result<Self, Error> {
        let mut scripts = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "toml") {
                let text = fs::read_to_string(&path)?;
                let script: CardScript =
                    toml::from_str(&text).map_err(|source| Error::ScriptFile { path, source })?;
                scripts.push(script);
            }
        }
        Ok(ScriptPool::new(scripts))
    }

    pub fn get(&self, name: &str) -> Option<&Arc<CardScript>> {
        self.by_name.get(name)
    }

    /// The file name a card's script is saved under.
    pub fn file_name(card_name: &str) -> String {
        format!("{}.toml", slug(card_name))
    }

    /// Why `card` can't be played yet, if it can't.
    pub fn unplayable(&self, card: &Card) -> Option<String> {
        unplayable(card, self.get(&card.name).map(|s| s.as_ref()))
    }
}

/// Why `card` can't be played with `script`, if it can't.
///
/// A card is playable when every part of it is supported: a basic land with
/// no other abilities, a creature whose only abilities are native keywords,
/// or any card with a script, as long as it isn't a kind of card or cost the
/// engine doesn't handle yet.
pub fn unplayable(card: &Card, script: Option<&CardScript>) -> Option<String> {
    for t in [CardType::Planeswalker, CardType::Battle] {
        if card.is(t) {
            return Some(format!("{t:?} cards aren't supported yet"));
        }
    }
    if card
        .back_face
        .as_ref()
        .is_some_and(|b| b.kind == DoubleFaced::Nonmodal)
    {
        return Some("transforming double-faced cards aren't supported yet".into());
    }
    // CR 208.2: a star needs a characteristic-defining ability.
    if [&card.power, &card.toughness]
        .iter()
        .any(|s| matches!(s, Some(Stat::Variable(_))))
    {
        return Some("variable power or toughness isn't supported yet".into());
    }
    if let Some(s) = card.mana_cost.0.iter().find(|s| {
        matches!(
            s,
            ManaSymbol::X
                | ManaSymbol::Snow
                | ManaSymbol::Phyrexian(_)
                | ManaSymbol::PhyrexianHybrid(..)
        )
    }) {
        return Some(format!("{s} in mana costs isn't supported yet"));
    }
    for ability in &card.abilities {
        if let Ability::Keyword { keyword, text } = ability
            && !NATIVE_KEYWORDS.contains(keyword)
        {
            return Some(format!("keyword {text:?} isn't supported yet"));
        }
    }
    if script.is_some() {
        return None;
    }
    let other_abilities = card
        .abilities
        .iter()
        .any(|a| !matches!(a, Ability::Keyword { .. }));
    if card.is(CardType::Land) {
        let basic_type = ["Plains", "Island", "Swamp", "Mountain", "Forest"]
            .iter()
            .any(|t| card.type_line.has_subtype(t));
        if !basic_type || other_abilities {
            return Some("this land needs a script".into());
        }
        return None;
    }
    if card.type_line.is_instant_or_sorcery() {
        return Some("this spell needs a script".into());
    }
    if other_abilities {
        return Some("this card's abilities need a script".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::testing::card;
    use crate::game::testing::creature;

    // Every script in scripts/ parses, and is saved under its card's name.
    #[test]
    fn repository_scripts_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts");
        let pool = ScriptPool::load(&dir).unwrap();
        assert!(!pool.by_name.is_empty());
        for name in pool.by_name.keys() {
            assert!(
                dir.join(ScriptPool::file_name(name)).exists(),
                "the script for {name:?} should be {}",
                ScriptPool::file_name(name)
            );
        }
    }

    // A script with every kind of ability parses.
    #[test]
    fn parses_a_full_script() {
        let script: CardScript = toml::from_str(
            r#"
            name = "Test Card"

            [[triggered]]
            on = { event = "enters", object = "self" }
            effect = "draw"

            [[activated]]
            cost = { mana = "{1}{R}", tap = true }
            effect = "deal_damage"
            amount = 1
            target = { kind = "any" }

            [[mana]]
            produces = ["{R}", "{G}"]

            [[static]]
            affects = { each = "creature", controller = "you", other = true }
            power = 1
            toughness = 1
            "#,
        )
        .unwrap();
        assert_eq!(script.triggered.len(), 1);
        assert_eq!(script.activated[0].cost.mana.to_string(), "{1}{R}");
        assert!(script.activated[0].cost.tap);
        assert!(matches!(
            script.activated[0].effect,
            Effect::DealDamage { amount: 1, .. }
        ));
        assert_eq!(script.mana[0].produces.len(), 2);
        assert_eq!(script.statics[0].power, 1);
    }

    #[test]
    fn scripts_round_trip() {
        let text = r#"
            name = "Shock"
            [spell]
            effect = "deal_damage"
            amount = 2
            target = { kind = "any" }
        "#;
        let script: CardScript = toml::from_str(text).unwrap();
        let again: CardScript = toml::from_str(&toml::to_string(&script).unwrap()).unwrap();
        assert_eq!(script, again);
    }

    // CR 305.6: basic land types give lands their mana abilities.
    #[test]
    fn basic_lands_need_no_script() {
        let forest = card("Forest", "1", "", "Basic Land — Forest", "({T}: Add {G}.)");
        assert_eq!(unplayable(&forest, None), None);
        let dual = card("Grove", "2", "", "Land", "{T}: Add {G} or {U}.");
        assert!(unplayable(&dual, None).is_some());
    }

    #[test]
    fn vanilla_and_keyword_creatures_need_no_script() {
        let bear = creature("Bear", "{1}{G}", 2, 2, "");
        assert_eq!(unplayable(&bear, None), None);
        let bird = creature("Bird", "{W}", 1, 1, "Flying, vigilance");
        assert_eq!(unplayable(&bird, None), None);
        let elf = creature("Elf", "{G}", 1, 1, "{T}: Add {G}.");
        assert!(unplayable(&elf, None).is_some());
        // The test helper's creatures have toughness 1+*.
        let goyf = card("Goyf", "5", "{1}{G}", "Creature — Lhurgoyf", "");
        assert!(unplayable(&goyf, Some(&CardScript::default())).is_some());
        let warded = creature("Ward Guy", "{G}", 1, 1, "Ward {2}");
        assert!(unplayable(&warded, Some(&CardScript::default())).is_some());
    }

    #[test]
    fn spells_need_a_script() {
        let bolt = card(
            "Bolt",
            "1",
            "{R}",
            "Instant",
            "Deal 3 damage to any target.",
        );
        assert!(unplayable(&bolt, None).is_some());
        assert_eq!(unplayable(&bolt, Some(&CardScript::default())), None);
        let fireball = card("Fireball", "2", "{X}{R}", "Sorcery", "Deal X damage.");
        assert!(unplayable(&fireball, Some(&CardScript::default())).is_some());
    }
}
