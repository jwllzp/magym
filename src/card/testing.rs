//! Card constructors shared by unit tests.

use super::{Card, CardType, ManaCost, Rarity, TypeLine, parse_abilities};

/// A card from the test set `tst`. Creatures get power 2 and toughness
/// `1+*`; colors and color identity come from the mana cost.
pub fn card(name: &str, collector_number: &str, cost: &str, type_line: &str, text: &str) -> Card {
    let type_line: TypeLine = type_line.parse().unwrap();
    let mana_cost: ManaCost = cost.parse().unwrap();
    let creature = type_line.is(CardType::Creature);
    Card {
        name: name.into(),
        mana_value: mana_cost.mana_value(),
        colors: mana_cost.colors(),
        color_identity: mana_cost.colors(),
        abilities: parse_abilities(text, type_line.is_instant_or_sorcery()),
        mana_cost,
        type_line,
        power: creature.then(|| "2".parse().unwrap()),
        toughness: creature.then(|| "1+*".parse().unwrap()),
        loyalty: None,
        defense: None,
        rarity: Rarity::Uncommon,
        set: "tst".into(),
        collector_number: collector_number.into(),
        id: "00000000-0000-0000-0000-000000000001".into(),
        oracle_id: "00000000-0000-0000-0000-000000000002".into(),
        image_url: None,
        oracle_text: text.into(),
    }
}
