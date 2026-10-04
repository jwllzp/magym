//! Downloads cards from the Scryfall API (https://scryfall.com/docs/api) and
//! converts them into [`Card`]s.

use std::thread;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::card::{Card, CardType, Color, ManaCost, Rarity, Stat, TypeLine, parse_abilities};
use crate::error::Error;

/// Set code of Reality Fracture.
pub const REALITY_FRACTURE: &str = "fra";

const API: &str = "https://api.scryfall.com";
const USER_AGENT: &str = concat!("magym/", env!("CARGO_PKG_VERSION"));
/// Scryfall asks for 50–100 ms between requests.
const REQUEST_DELAY: Duration = Duration::from_millis(100);

/// The subset of Scryfall's card object that we use.
#[derive(Debug, Deserialize)]
struct ScryfallCard {
    id: String,
    oracle_id: Option<String>,
    name: String,
    layout: String,
    mana_cost: Option<String>,
    type_line: Option<String>,
    oracle_text: Option<String>,
    power: Option<String>,
    toughness: Option<String>,
    loyalty: Option<String>,
    defense: Option<String>,
    colors: Option<Vec<String>>,
    #[serde(default)]
    color_identity: Vec<String>,
    rarity: String,
    set: String,
    collector_number: String,
    image_uris: Option<ImageUris>,
    card_faces: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ImageUris {
    normal: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScryfallList {
    data: Vec<ScryfallCard>,
    #[serde(default)]
    has_more: bool,
    next_page: Option<String>,
    total_cards: Option<usize>,
}

fn parse_colors(colors: &[String]) -> Result<Vec<Color>, Error> {
    let mut colors = colors
        .iter()
        .map(|c| c.parse())
        .collect::<Result<Vec<Color>, _>>()?;
    colors.sort();
    Ok(colors)
}

impl TryFrom<ScryfallCard> for Card {
    type Error = Error;

    fn try_from(c: ScryfallCard) -> Result<Self, Self::Error> {
        // Only single-faced cards are modeled for now.
        if c.layout != "normal" || c.card_faces.is_some() {
            return Err(Error::Unsupported(c.layout));
        }

        let type_line: TypeLine = c
            .type_line
            .as_deref()
            .ok_or(Error::MissingField("type_line"))?
            .parse()?;
        let mana_cost: ManaCost = c.mana_cost.as_deref().unwrap_or("").parse()?;
        let oracle_text = c.oracle_text.unwrap_or_default();
        let stat = |s: Option<String>| s.map(|s| s.parse::<Stat>().unwrap_or_else(|e| match e {}));
        let (power, toughness) = (stat(c.power), stat(c.toughness));
        let (loyalty, defense) = (stat(c.loyalty), stat(c.defense));

        // CR 208.1, 209.1, 310.4a: these card types always print these numbers.
        if type_line.is(CardType::Creature) && (power.is_none() || toughness.is_none()) {
            return Err(Error::MissingField("power/toughness"));
        }
        if type_line.is(CardType::Planeswalker) && loyalty.is_none() {
            return Err(Error::MissingField("loyalty"));
        }
        if type_line.is(CardType::Battle) && defense.is_none() {
            return Err(Error::MissingField("defense"));
        }

        let colors = match &c.colors {
            Some(colors) => parse_colors(colors)?,
            None => mana_cost.colors(),
        };

        Ok(Card {
            oracle_id: c.oracle_id.ok_or(Error::MissingField("oracle_id"))?,
            id: c.id,
            name: c.name,
            mana_value: mana_cost.mana_value(),
            abilities: parse_abilities(&oracle_text, type_line.is_instant_or_sorcery()),
            mana_cost,
            type_line,
            oracle_text,
            power,
            toughness,
            loyalty,
            defense,
            colors,
            color_identity: parse_colors(&c.color_identity)?,
            rarity: c.rarity.parse::<Rarity>()?,
            set: c.set,
            collector_number: c.collector_number,
            image_url: c.image_uris.and_then(|i| i.normal),
        })
    }
}

/// A card that was downloaded but not converted, and why.
#[derive(Debug, Clone)]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct FetchReport {
    pub cards: Vec<Card>,
    pub skipped: Vec<Skipped>,
    /// Total number of cards Scryfall reported for the query.
    pub total_cards: usize,
}

/// A Magic set as listed by Scryfall (https://scryfall.com/docs/api/sets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetInfo {
    pub code: String,
    pub name: String,
    /// Scryfall's set type, e.g. `expansion`, `core`, `masters`.
    pub set_type: String,
    /// `YYYY-MM-DD`; may be in the future for announced sets.
    pub released_at: Option<String>,
    pub card_count: usize,
    pub digital: bool,
}

/// Set types whose cards are sold as playable paper boosters or decks.
pub const PLAYABLE_SET_TYPES: &[&str] = &[
    "core",
    "expansion",
    "masters",
    "draft_innovation",
    "commander",
];

impl SetInfo {
    pub fn is_playable(&self) -> bool {
        !self.digital && PLAYABLE_SET_TYPES.contains(&self.set_type.as_str())
    }
}

#[derive(Debug, Deserialize)]
struct SetList {
    data: Vec<SetInfo>,
}

fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, Error> {
    Ok(ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .call()?
        .body_mut()
        .read_json()?)
}

/// Lists every set on Scryfall, newest first.
pub fn list_sets() -> Result<Vec<SetInfo>, Error> {
    Ok(get_json::<SetList>(&format!("{API}/sets"))?.data)
}

/// Set codes are short alphanumeric strings such as `fra` or `m21`.
fn validate_set_code(code: &str) -> Result<(), Error> {
    if code.is_empty() || code.len() > 8 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::SetCode(code.to_string()));
    }
    Ok(())
}

/// Downloads every card (one printing per card) of the set with the given code.
///
/// Returns [`Error::UnknownSet`] when Scryfall has no cards for the code.
pub fn fetch_set(code: &str) -> Result<FetchReport, Error> {
    validate_set_code(code)?;
    let code = code.to_ascii_lowercase();
    let mut report = FetchReport::default();
    let mut url = format!("{API}/cards/search?q=e%3A{code}&unique=cards&order=set");
    let mut first = true;
    loop {
        let page: ScryfallList = match get_json(&url) {
            // Scryfall answers 404 when a search matches no cards.
            Err(Error::Http(e)) if first && matches!(*e, ureq::Error::StatusCode(404)) => {
                return Err(Error::UnknownSet(code));
            }
            result => result?,
        };
        first = false;
        if let Some(total) = page.total_cards {
            report.total_cards = total;
        }
        for raw in page.data {
            let name = raw.name.clone();
            match Card::try_from(raw) {
                Ok(card) => report.cards.push(card),
                Err(e) => report.skipped.push(Skipped {
                    name,
                    reason: e.to_string(),
                }),
            }
        }
        match page.next_page {
            Some(next) if page.has_more => {
                url = next;
                thread::sleep(REQUEST_DELAY);
            }
            _ => break,
        }
    }
    Ok(report)
}

pub fn fetch_reality_fracture() -> Result<FetchReport, Error> {
    fetch_set(REALITY_FRACTURE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Ability, Keyword, ManaSymbol, Supertype};
    use serde_json::json;

    /// Builds a Scryfall card object (see https://scryfall.com/docs/api/cards)
    /// with the given card-specific fields.
    fn scryfall(fields: serde_json::Value) -> ScryfallCard {
        let mut card = json!({
            "object": "card",
            "id": "00000000-0000-0000-0000-000000000001",
            "oracle_id": "00000000-0000-0000-0000-000000000002",
            "layout": "normal",
            "color_identity": [],
            "rarity": "common",
            "set": "tst",
            "collector_number": "1",
            "image_uris": { "normal": "https://example.com/card.jpg" },
        });
        card.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        serde_json::from_value(card).unwrap()
    }

    fn convert(fields: serde_json::Value) -> Result<Card, Error> {
        Card::try_from(scryfall(fields))
    }

    #[test]
    fn vanilla_creature() {
        let card = convert(json!({
            "name": "Grizzly Bears", "mana_cost": "{1}{G}", "type_line": "Creature — Bear",
            "oracle_text": "", "power": "2", "toughness": "2", "colors": ["G"], "color_identity": ["G"],
        }))
        .unwrap();
        assert_eq!(card.mana_value, 2);
        assert_eq!(card.colors, vec![Color::Green]);
        assert_eq!(card.power, Some(Stat::Fixed(2)));
        assert_eq!(card.toughness, Some(Stat::Fixed(2)));
        assert!(card.abilities.is_empty());
        assert_eq!(
            card.image_url.as_deref(),
            Some("https://example.com/card.jpg")
        );
    }

    #[test]
    fn instant() {
        let card = convert(json!({
            "name": "Lightning Bolt", "mana_cost": "{R}", "type_line": "Instant",
            "oracle_text": "Lightning Bolt deals 3 damage to any target.", "colors": ["R"],
        }))
        .unwrap();
        assert!(card.is(CardType::Instant));
        assert_eq!(card.power, None);
        assert!(matches!(card.abilities.as_slice(), [Ability::Spell { .. }]));
    }

    // CR 202.1b, 305.6: basic lands have no mana cost and an intrinsic mana ability.
    #[test]
    fn basic_land() {
        let card = convert(json!({
            "name": "Forest", "mana_cost": "", "type_line": "Basic Land — Forest",
            "oracle_text": "({T}: Add {G}.)", "colors": [], "color_identity": ["G"],
        }))
        .unwrap();
        assert!(card.mana_cost.is_empty());
        assert_eq!(card.mana_value, 0);
        assert!(card.colors.is_empty());
        assert_eq!(card.color_identity, vec![Color::Green]);
        assert!(card.type_line.has_supertype(Supertype::Basic));
    }

    // CR 209.1, 606: planeswalkers print loyalty and have loyalty abilities.
    #[test]
    fn planeswalker() {
        let card = convert(json!({
            "name": "Jace Beleren", "mana_cost": "{1}{U}{U}", "type_line": "Legendary Planeswalker — Jace",
            "oracle_text": "+2: Each player draws a card.\n−1: Target player draws a card.\n−10: Target player mills twenty cards.",
            "loyalty": "3", "colors": ["U"],
        }))
        .unwrap();
        assert_eq!(card.loyalty, Some(Stat::Fixed(3)));
        assert_eq!(card.abilities.len(), 3);
        assert!(
            card.abilities
                .iter()
                .all(|a| matches!(a, Ability::Activated { .. }))
        );
    }

    // CR 202.3e: X is 0 when computing mana value.
    #[test]
    fn x_spell() {
        let card = convert(json!({
            "name": "Fireball", "mana_cost": "{X}{R}", "type_line": "Sorcery",
            "oracle_text": "This spell costs {1} more to cast for each target beyond the first.\nFireball deals X damage divided evenly, rounded down, among any number of targets.",
            "colors": ["R"],
        }))
        .unwrap();
        assert_eq!(
            card.mana_cost.0,
            vec![ManaSymbol::X, ManaSymbol::Colored(Color::Red)]
        );
        assert_eq!(card.mana_value, 1);
    }

    // CR 208.2a: star power/toughness defined by a characteristic-defining ability.
    #[test]
    fn star_power_toughness() {
        let card = convert(json!({
            "name": "Tarmogoyf", "mana_cost": "{1}{G}", "type_line": "Creature — Lhurgoyf",
            "oracle_text": "Tarmogoyf's power is equal to the number of card types among cards in all graveyards and its toughness is equal to that number plus 1.",
            "power": "*", "toughness": "1+*", "colors": ["G"],
        }))
        .unwrap();
        assert_eq!(card.power, Some(Stat::Variable("*".into())));
        assert_eq!(card.toughness, Some(Stat::Variable("1+*".into())));
    }

    // CR 301.7a: Vehicles print power and toughness although they aren't creatures.
    #[test]
    fn vehicle_keeps_printed_stats() {
        let card = convert(json!({
            "name": "Smuggler's Copter", "mana_cost": "{2}", "type_line": "Artifact — Vehicle",
            "oracle_text": "Flying\nWhenever this Vehicle attacks or blocks, you may draw a card. If you do, discard a card.\nCrew 1",
            "power": "3", "toughness": "3", "colors": [],
        }))
        .unwrap();
        assert!(!card.is(CardType::Creature));
        assert_eq!(card.power, Some(Stat::Fixed(3)));
        assert!(card.has_keyword(&Keyword::Flying));
        assert!(card.has_keyword(&Keyword::Other("Crew".into())));
    }

    // CR 310.4a: battles print a defense number.
    #[test]
    fn battle() {
        let card = convert(json!({
            "name": "Test Siege", "mana_cost": "{2}{R}", "type_line": "Battle — Siege",
            "oracle_text": "When this battle enters, it deals 2 damage to each creature.",
            "defense": "5", "colors": ["R"],
        }))
        .unwrap();
        assert_eq!(card.defense, Some(Stat::Fixed(5)));
    }

    // CR 208.1, 209.1, 310.4a: missing printed numbers are rejected.
    #[test]
    fn missing_printed_numbers() {
        let creature =
            convert(json!({ "name": "C", "mana_cost": "{1}", "type_line": "Creature — Bear" }));
        assert!(matches!(
            creature,
            Err(Error::MissingField("power/toughness"))
        ));
        let walker =
            convert(json!({ "name": "P", "mana_cost": "{1}", "type_line": "Planeswalker — Jace" }));
        assert!(matches!(walker, Err(Error::MissingField("loyalty"))));
        let battle =
            convert(json!({ "name": "B", "mana_cost": "{1}", "type_line": "Battle — Siege" }));
        assert!(matches!(battle, Err(Error::MissingField("defense"))));
    }

    #[test]
    fn colors_fall_back_to_mana_cost() {
        let card = convert(json!({
            "name": "Hybrid", "mana_cost": "{W/U}", "type_line": "Instant", "oracle_text": "Draw a card.",
        }))
        .unwrap();
        assert_eq!(card.colors, vec![Color::White, Color::Blue]);
    }

    // Multi-faced cards (CR 709–712) are out of scope.
    #[test]
    fn multi_faced_cards_are_unsupported() {
        let card = convert(json!({
            "name": "Front // Back", "layout": "transform", "type_line": "Creature — Human // Creature — Werewolf",
            "card_faces": [{ "name": "Front" }, { "name": "Back" }],
        }));
        assert!(matches!(card, Err(Error::Unsupported(layout)) if layout == "transform"));
    }

    #[test]
    fn set_codes() {
        for ok in ["fra", "M21", "pf27", "10e"] {
            assert!(validate_set_code(ok).is_ok(), "{ok}");
        }
        for bad in ["", "e:fra", "fra&q=x", "fr a", "toolongcode"] {
            assert!(validate_set_code(bad).is_err(), "{bad}");
        }
    }

    // Shape of a set object per https://scryfall.com/docs/api/sets.
    #[test]
    fn set_info_from_scryfall() {
        let set: SetInfo = serde_json::from_value(json!({
            "object": "set",
            "code": "m21",
            "name": "Core Set 2021",
            "set_type": "core",
            "released_at": "2020-07-03",
            "card_count": 397,
            "digital": false,
            "icon_svg_uri": "https://svgs.scryfall.io/sets/m21.svg"
        }))
        .unwrap();
        assert_eq!(set.code, "m21");
        assert_eq!(set.card_count, 397);
        assert!(set.is_playable());

        let token = SetInfo {
            set_type: "token".into(),
            ..set.clone()
        };
        assert!(!token.is_playable());
        let digital = SetInfo {
            digital: true,
            ..set
        };
        assert!(!digital.is_playable());
    }

    // Requires network access: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn list_sets_live() {
        let sets = list_sets().unwrap();
        assert!(sets.iter().any(|s| s.code == REALITY_FRACTURE));
    }

    // Requires network access: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn unknown_set_live() {
        assert!(matches!(fetch_set("zzzz"), Err(Error::UnknownSet(_))));
    }

    // Requires network access: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn fetch_reality_fracture_live() {
        let report = fetch_reality_fracture().unwrap();
        assert!(report.total_cards > 0);
        assert_eq!(
            report.cards.len() + report.skipped.len(),
            report.total_cards
        );
        assert!(report.cards.iter().all(|c| c.set == REALITY_FRACTURE));
    }
}
