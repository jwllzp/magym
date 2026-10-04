//! Deck construction rules (CR 100.2, 100.4, 903.3–903.5).

use thiserror::Error;

use super::{CardRef, Deck, Entry, Format};
use crate::card::{Card, CardType, Color, Supertype};
use crate::pool::CardPool;

/// A way a deck breaks the deck construction rules of its format.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Violation {
    #[error("{} ({} {}) is not in the card pool", .0.name, .0.set, .0.collector_number)]
    UnknownCard(CardRef),
    #[error("the deck has {actual} cards, but needs at least {min}")]
    TooFewCards { min: u32, actual: u32 },
    #[error("the deck has {actual} cards including the commander, but needs exactly {expected}")]
    WrongSize { expected: u32, actual: u32 },
    #[error("{name}: {actual} copies, but at most {max} are allowed")]
    TooManyCopies { name: String, max: u32, actual: u32 },
    #[error("the sideboard has {actual} cards, but can have at most {max}")]
    SideboardTooLarge { max: u32, actual: u32 },
    #[error("{0:?} decks don't use sideboards")]
    SideboardNotAllowed(Format),
    #[error("a Commander deck needs a commander")]
    MissingCommander,
    #[error("{0:?} decks don't have a commander")]
    CommanderNotAllowed(Format),
    #[error("{0} can't be a commander")]
    InvalidCommander(String),
    #[error("{0} is outside the commander's color identity")]
    OutsideColorIdentity(String),
}

impl Deck {
    /// Checks the deck against the construction rules of its format.
    /// Returns every violation found; an empty list means the deck is legal.
    ///
    /// Not checked yet: interchangeable names (CR 201.3b) and Limited's limit
    /// of as many copies as the product contained (CR 100.2b), which need
    /// data the card pool doesn't have.
    pub fn validate(&self, pool: &CardPool) -> Vec<Violation> {
        let mut violations = Vec::new();
        let mut resolve = |card: &CardRef| {
            let found = pool.get(card);
            if found.is_none() {
                violations.push(Violation::UnknownCard(card.clone()));
            }
            found
        };
        let main = resolved(&self.main, &mut resolve);
        let sideboard = resolved(&self.sideboard, &mut resolve);
        let commander = self.commander.as_ref().map(&mut resolve);

        let main_count = self.main_count();
        let sideboard_count = self.sideboard_count();
        match self.format {
            Format::Constructed => {
                // CR 100.2a
                if main_count < 60 {
                    violations.push(Violation::TooFewCards {
                        min: 60,
                        actual: main_count,
                    });
                }
                // CR 100.4a
                if sideboard_count > 15 {
                    violations.push(Violation::SideboardTooLarge {
                        max: 15,
                        actual: sideboard_count,
                    });
                }
                // CR 100.4a: the four-card limit covers deck and sideboard together.
                let all: Vec<_> = main.iter().chain(&sideboard).copied().collect();
                check_copies(&all, 4, &mut violations);
            }
            Format::Limited => {
                // CR 100.2b
                if main_count < 40 {
                    violations.push(Violation::TooFewCards {
                        min: 40,
                        actual: main_count,
                    });
                }
            }
            Format::Commander => {
                // CR 903.5a
                let actual = main_count + u32::from(self.commander.is_some());
                if actual != 100 {
                    violations.push(Violation::WrongSize {
                        expected: 100,
                        actual,
                    });
                }
                // CR 903.5e
                if !self.sideboard.is_empty() {
                    violations.push(Violation::SideboardNotAllowed(self.format));
                }
                match commander {
                    None => violations.push(Violation::MissingCommander),
                    Some(None) => {}
                    Some(Some(commander)) => {
                        if !can_be_commander(commander) {
                            violations.push(Violation::InvalidCommander(commander.name.clone()));
                        }
                        for (_, card) in &main {
                            if !within_identity(card, &commander.color_identity) {
                                violations.push(Violation::OutsideColorIdentity(card.name.clone()));
                            }
                        }
                    }
                }
                // CR 903.5b: singleton, and the commander counts.
                let mut all = main.clone();
                if let Some(Some(commander)) = commander {
                    all.push((1, commander));
                }
                check_copies(&all, 1, &mut violations);
            }
        }
        if self.format != Format::Commander && self.commander.is_some() {
            violations.push(Violation::CommanderNotAllowed(self.format));
        }
        violations
    }
}

/// The entries whose card is in the pool, with their cards.
fn resolved<'a>(
    entries: &[Entry],
    resolve: &mut impl FnMut(&CardRef) -> Option<&'a Card>,
) -> Vec<(u32, &'a Card)> {
    entries
        .iter()
        .filter_map(|e| resolve(&e.card).map(|card| (e.count, card)))
        .collect()
}

/// Flags every English name with more copies than its limit, counting all
/// printings of that name together.
fn check_copies(cards: &[(u32, &Card)], default_max: u32, violations: &mut Vec<Violation>) {
    let mut totals: Vec<(&Card, u32)> = Vec::new();
    for &(count, card) in cards {
        match totals.iter_mut().find(|(c, _)| c.name == card.name) {
            Some((_, total)) => *total += count,
            None => totals.push((card, count)),
        }
    }
    for (card, actual) in totals {
        if let Some(max) = copy_limit(card, default_max)
            && actual > max
        {
            violations.push(Violation::TooManyCopies {
                name: card.name.clone(),
                max,
                actual,
            });
        }
    }
}

/// How many copies of `card` a deck may contain, or `None` for any number.
fn copy_limit(card: &Card, default_max: u32) -> Option<u32> {
    // CR 100.2a, 903.5b: basic lands are exempt.
    if card.type_line.has_supertype(Supertype::Basic) {
        return None;
    }
    // CR 113.6n: abilities like "A deck can have any number of cards named
    // Relentless Rats." override the limit.
    let text = &card.oracle_text;
    if text.contains("A deck can have any number of cards named") {
        return None;
    }
    if let Some(rest) = text.split("A deck can have up to ").nth(1)
        && let Some((number, _)) = rest.split_once(" cards named")
        && let Some(max) = number_word(number)
    {
        return Some(max);
    }
    Some(default_max)
}

fn number_word(word: &str) -> Option<u32> {
    let words = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    words
        .iter()
        .position(|w| *w == word)
        .map(|i| i as u32 + 1)
        .or_else(|| word.parse().ok())
}

/// CR 903.3, 903.3a
fn can_be_commander(card: &Card) -> bool {
    if card.oracle_text.contains("can be your commander") {
        return true;
    }
    card.type_line.has_supertype(Supertype::Legendary)
        && (card.is(CardType::Creature)
            || card.type_line.has_subtype("Vehicle")
            || (card.type_line.has_subtype("Spacecraft") && card.power.is_some()))
}

/// CR 903.5c, 903.5d
fn within_identity(card: &Card, identity: &[Color]) -> bool {
    let basic_land_colors = card
        .type_line
        .subtypes
        .iter()
        .filter_map(|t| match t.as_str() {
            "Plains" => Some(Color::White),
            "Island" => Some(Color::Blue),
            "Swamp" => Some(Color::Black),
            "Mountain" => Some(Color::Red),
            "Forest" => Some(Color::Green),
            _ => None,
        });
    card.color_identity
        .iter()
        .copied()
        .chain(basic_land_colors)
        .all(|c| identity.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::testing::card;

    fn numbered(i: usize) -> String {
        format!("{}", 100 + i)
    }

    /// `n` different red instants, collector numbers 100 and up.
    fn spells(n: usize) -> Vec<Card> {
        (0..n)
            .map(|i| card(&format!("Spell {i}"), &numbered(i), "{R}", "Instant", ""))
            .collect()
    }

    fn mountain() -> Card {
        card("Mountain", "1", "", "Basic Land — Mountain", "")
    }

    fn legend() -> Card {
        card(
            "Red Legend",
            "2",
            "{2}{R}",
            "Legendary Creature — Goblin",
            "",
        )
    }

    /// A deck with one copy of each card, plus a pool holding them.
    fn deck_of(format: Format, cards: &[Card]) -> (Deck, Vec<Card>) {
        let mut deck = Deck::new("Test", format);
        for c in cards {
            deck.add(c, 1);
        }
        (deck, cards.to_vec())
    }

    fn check(deck: &Deck, mut cards: Vec<Card>, extra: &[Card]) -> Vec<Violation> {
        cards.extend_from_slice(extra);
        deck.validate(&CardPool::new(cards))
    }

    // CR 100.2a: a constructed deck has a minimum deck size of 60 cards.
    #[test]
    fn constructed_minimum_is_60() {
        let (deck, pool) = deck_of(Format::Constructed, &spells(59));
        assert_eq!(
            check(&deck, pool, &[]),
            [Violation::TooFewCards {
                min: 60,
                actual: 59
            }]
        );
        let (deck, pool) = deck_of(Format::Constructed, &spells(60));
        assert_eq!(check(&deck, pool, &[]), []);
    }

    // CR 100.5: there is no maximum deck size for non-Commander decks.
    #[test]
    fn constructed_has_no_maximum() {
        let (deck, pool) = deck_of(Format::Constructed, &spells(250));
        assert_eq!(check(&deck, pool, &[]), []);
    }

    // CR 100.2a: no more than four of any card with a particular English name.
    #[test]
    fn constructed_allows_four_copies() {
        let (mut deck, pool) = deck_of(Format::Constructed, &spells(57));
        deck.add(&pool[0], 3);
        assert_eq!(check(&deck, pool.clone(), &[]), []);
        deck.add(&pool[0], 1);
        assert_eq!(
            check(&deck, pool, &[]),
            [Violation::TooManyCopies {
                name: "Spell 0".into(),
                max: 4,
                actual: 5
            }]
        );
    }

    // CR 100.2a: the limit is per English name, so different printings add up.
    #[test]
    fn copies_count_across_printings() {
        let (mut deck, pool) = deck_of(Format::Constructed, &spells(60));
        let reprint = card("Spell 0", "999", "{R}", "Instant", "");
        deck.add(&reprint, 4);
        assert!(
            check(&deck, pool, &[reprint]).contains(&Violation::TooManyCopies {
                name: "Spell 0".into(),
                max: 4,
                actual: 5
            })
        );
    }

    // CR 100.2a: a constructed deck may contain any number of basic land cards.
    #[test]
    fn basic_lands_are_unlimited() {
        let mut deck = Deck::new("Mono-Red", Format::Constructed);
        deck.add(&mountain(), 60);
        assert_eq!(check(&deck, vec![mountain()], &[]), []);
    }

    // CR 205.4c: a land with a basic land type but no "basic" supertype is
    // nonbasic, so the four-card limit applies.
    #[test]
    fn nonbasic_with_basic_land_type_is_limited() {
        let dual = card("Dual", "3", "", "Land — Mountain Forest", "");
        let (mut deck, pool) = deck_of(Format::Constructed, &spells(55));
        deck.add(&dual, 5);
        assert_eq!(
            check(&deck, pool, &[dual]),
            [Violation::TooManyCopies {
                name: "Dual".into(),
                max: 4,
                actual: 5
            }]
        );
    }

    // CR 113.6n: an ability that modifies deck construction rules applies.
    #[test]
    fn deck_construction_abilities_override_the_limit() {
        let rats = card(
            "Rats",
            "4",
            "{1}{B}{B}",
            "Creature — Rat",
            "A deck can have any number of cards named Rats.",
        );
        let dwarves = card(
            "Dwarves",
            "5",
            "{R}",
            "Creature — Dwarf",
            "A deck can have up to seven cards named Dwarves.",
        );
        let mut deck = Deck::new("Swarm", Format::Constructed);
        deck.add(&rats, 52);
        deck.add(&dwarves, 8);
        assert_eq!(
            check(&deck, vec![rats, dwarves], &[]),
            [Violation::TooManyCopies {
                name: "Dwarves".into(),
                max: 7,
                actual: 8
            }]
        );
    }

    // CR 100.4a: a constructed sideboard has at most fifteen cards.
    #[test]
    fn constructed_sideboard_is_at_most_15() {
        let cards = spells(76);
        let (mut deck, pool) = deck_of(Format::Constructed, &cards[..60]);
        for c in &cards[60..] {
            deck.add_to_sideboard(c, 1);
        }
        assert_eq!(
            check(&deck, pool, &cards[60..]),
            [Violation::SideboardTooLarge {
                max: 15,
                actual: 16
            }]
        );
    }

    // CR 100.4a: the four-card limit applies to deck and sideboard combined.
    #[test]
    fn copy_limit_includes_sideboard() {
        let (mut deck, pool) = deck_of(Format::Constructed, &spells(60));
        deck.add(&pool[0], 2);
        deck.add_to_sideboard(&pool[0], 2);
        assert_eq!(
            check(&deck, pool, &[]),
            [Violation::TooManyCopies {
                name: "Spell 0".into(),
                max: 4,
                actual: 5
            }]
        );
    }

    // CR 100.2b: a limited deck has a minimum deck size of 40 cards and may
    // have as many duplicates as the product included.
    #[test]
    fn limited_minimum_is_40() {
        let (mut deck, pool) = deck_of(Format::Limited, &spells(39));
        deck.add(&pool[0], 5);
        assert_eq!(check(&deck, pool.clone(), &[]), []);
        let (deck, pool) = deck_of(Format::Limited, &spells(39));
        assert_eq!(
            check(&deck, pool, &[]),
            [Violation::TooFewCards {
                min: 40,
                actual: 39
            }]
        );
    }

    // CR 100.4b: in limited, the rest of the pool is the sideboard, whatever its size.
    #[test]
    fn limited_sideboard_is_unlimited() {
        let cards = spells(100);
        let (mut deck, pool) = deck_of(Format::Limited, &cards[..40]);
        for c in &cards[40..] {
            deck.add_to_sideboard(c, 1);
        }
        assert_eq!(check(&deck, pool, &cards[40..]), []);
    }

    fn commander_deck(main: &[Card]) -> (Deck, Vec<Card>) {
        let (mut deck, mut pool) = deck_of(Format::Commander, main);
        deck.set_commander(&legend());
        pool.push(legend());
        (deck, pool)
    }

    // CR 903.5a: exactly 100 cards, including the commander.
    #[test]
    fn commander_deck_is_exactly_100_with_commander() {
        let (deck, pool) = commander_deck(&spells(99));
        assert_eq!(check(&deck, pool, &[]), []);
        let (deck, pool) = commander_deck(&spells(100));
        assert_eq!(
            check(&deck, pool, &[]),
            [Violation::WrongSize {
                expected: 100,
                actual: 101
            }]
        );
    }

    // CR 903.3: each Commander deck has a commander.
    #[test]
    fn commander_deck_needs_a_commander() {
        let (deck, pool) = deck_of(Format::Commander, &spells(99));
        assert_eq!(
            check(&deck, pool, &[]),
            [
                Violation::WrongSize {
                    expected: 100,
                    actual: 99
                },
                Violation::MissingCommander
            ]
        );
    }

    // CR 903.5b: other than basic lands, each card has a different English
    // name, the commander included.
    #[test]
    fn commander_is_singleton() {
        let (mut deck, pool) = commander_deck(&spells(97));
        deck.add(&pool[0], 1);
        deck.add(&legend(), 1);
        let violations = check(&deck, pool, &[]);
        assert!(violations.contains(&Violation::TooManyCopies {
            name: "Spell 0".into(),
            max: 1,
            actual: 2
        }));
        assert!(violations.contains(&Violation::TooManyCopies {
            name: "Red Legend".into(),
            max: 1,
            actual: 2
        }));
        let (mut deck, pool) = commander_deck(&spells(9));
        deck.add(&mountain(), 90);
        assert_eq!(check(&deck, pool, &[mountain()]), []);
    }

    // CR 903.3: the commander is a legendary creature, a legendary Vehicle, or
    // a legendary Spacecraft with a power/toughness box.
    #[test]
    fn commander_must_be_legendary_creature_vehicle_or_spacecraft() {
        let mut cases = vec![
            (card("Hero", "10", "{R}", "Creature — Human", ""), false),
            (
                card("Saga Queen", "11", "{R}", "Legendary Enchantment", ""),
                false,
            ),
            (
                card("Chariot", "12", "{R}", "Legendary Artifact — Vehicle", ""),
                true,
            ),
            (
                card("Ship", "13", "{R}", "Legendary Artifact — Spacecraft", ""),
                false,
            ),
            (legend(), true),
        ];
        let mut ship_with_box = card(
            "Big Ship",
            "14",
            "{R}",
            "Legendary Artifact — Spacecraft",
            "",
        );
        ship_with_box.power = Some("4".parse().unwrap());
        ship_with_box.toughness = Some("4".parse().unwrap());
        cases.push((ship_with_box, true));
        for (commander, valid) in cases {
            let (mut deck, pool) = deck_of(Format::Commander, &spells(99));
            deck.set_commander(&commander);
            let invalid = Violation::InvalidCommander(commander.name.clone());
            assert_eq!(
                !check(&deck, pool, std::slice::from_ref(&commander)).contains(&invalid),
                valid,
                "{}",
                commander.name
            );
        }
    }

    // CR 903.3a: a card that says it can be your commander can be one.
    #[test]
    fn can_be_your_commander() {
        let walker = card(
            "Walker",
            "15",
            "{2}{R}",
            "Legendary Planeswalker — Chandra",
            "Walker can be your commander.",
        );
        let (mut deck, pool) = deck_of(Format::Commander, &spells(99));
        deck.set_commander(&walker);
        assert_eq!(check(&deck, pool, &[walker]), []);
    }

    // CR 903.5c: every card's color identity is within the commander's.
    #[test]
    fn cards_must_match_color_identity() {
        let blue = card("Blue Spell", "16", "{U}", "Instant", "");
        let (deck, pool) = commander_deck(&[spells(98), vec![blue]].concat());
        assert_eq!(
            check(&deck, pool, &[]),
            [Violation::OutsideColorIdentity("Blue Spell".into())]
        );
    }

    // CR 903.5c: colorless cards fit any commander.
    #[test]
    fn colorless_cards_fit_any_commander() {
        let rock = card("Rock", "17", "{2}", "Artifact", "");
        let (deck, pool) = commander_deck(&[spells(98), vec![rock]].concat());
        assert_eq!(check(&deck, pool, &[]), []);
    }

    // CR 903.5d: a card with a basic land type can be included only if each
    // color of mana it could produce is in the commander's color identity.
    #[test]
    fn basic_land_types_count_toward_identity() {
        let mut dual = card("Dual", "18", "", "Land — Mountain Forest", "");
        dual.color_identity = vec![];
        let (deck, pool) = commander_deck(&[spells(98), vec![dual]].concat());
        assert_eq!(
            check(&deck, pool, &[]),
            [Violation::OutsideColorIdentity("Dual".into())]
        );
    }

    // CR 903.5e: Commander games do not use sideboards.
    #[test]
    fn commander_has_no_sideboard() {
        let (mut deck, pool) = commander_deck(&spells(99));
        let extra = spells(100).pop().unwrap();
        deck.add_to_sideboard(&extra, 1);
        assert_eq!(
            check(&deck, pool, &[extra]),
            [Violation::SideboardNotAllowed(Format::Commander)]
        );
    }

    // Not a CR rule: only the Commander variant designates a commander
    // (CR 903.3), so a commander on another format's deck is a modeling mistake.
    #[test]
    fn only_commander_decks_have_a_commander() {
        let (mut deck, pool) = deck_of(Format::Constructed, &spells(60));
        deck.set_commander(&legend());
        assert_eq!(
            check(&deck, pool, &[legend()]),
            [Violation::CommanderNotAllowed(Format::Constructed)]
        );
    }

    #[test]
    fn unknown_cards_are_reported() {
        let (deck, mut pool) = deck_of(Format::Constructed, &spells(60));
        pool.remove(0);
        assert_eq!(
            check(&deck, pool, &[]),
            [Violation::UnknownCard(CardRef::from(&spells(1)[0]))]
        );
    }
}
