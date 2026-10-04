use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Supertypes (CR 205.4a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Supertype {
    Basic,
    Legendary,
    Ongoing,
    Snow,
    World,
}

/// Card types of traditional Magic cards (CR 205.2a, minus the nontraditional
/// conspiracy, dungeon, phenomenon, plane, scheme and vanguard types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardType {
    Artifact,
    Battle,
    Creature,
    Enchantment,
    Instant,
    Kindred,
    Land,
    Planeswalker,
    Sorcery,
}

/// Creature types that are two words long (CR 205.3m).
const TWO_WORD_SUBTYPES: &[&str] = &["Time Lord"];

/// A parsed type line such as `Legendary Creature — Elf Druid` (CR 205).
/// Serialized as the printed string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TypeLine {
    pub supertypes: Vec<Supertype>,
    pub types: Vec<CardType>,
    pub subtypes: Vec<String>,
}

impl TypeLine {
    pub fn is(&self, card_type: CardType) -> bool {
        self.types.contains(&card_type)
    }

    pub fn has_supertype(&self, supertype: Supertype) -> bool {
        self.supertypes.contains(&supertype)
    }

    pub fn has_subtype(&self, subtype: &str) -> bool {
        self.subtypes.iter().any(|s| s == subtype)
    }

    /// Instants and sorceries are spells whose text is spell abilities (CR 113.3a).
    pub fn is_instant_or_sorcery(&self) -> bool {
        self.is(CardType::Instant) || self.is(CardType::Sorcery)
    }
}

fn parse_subtypes(s: &str) -> Vec<String> {
    let mut subtypes = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        let two_word = TWO_WORD_SUBTYPES
            .iter()
            .find(|t| rest == **t || rest.starts_with(&format!("{t} ")));
        let len = match two_word {
            Some(t) => t.len(),
            None => rest.find(' ').unwrap_or(rest.len()),
        };
        subtypes.push(rest[..len].to_string());
        rest = rest[len..].trim_start();
    }
    subtypes
}

impl FromStr for TypeLine {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || Error::TypeLine(s.to_string());
        let (left, right) = match s.split_once('—') {
            Some((l, r)) => (l, r),
            None => (s, ""),
        };

        let mut supertypes = Vec::new();
        let mut types = Vec::new();
        for word in left.split_whitespace() {
            match word {
                "Basic" => supertypes.push(Supertype::Basic),
                "Legendary" => supertypes.push(Supertype::Legendary),
                "Ongoing" => supertypes.push(Supertype::Ongoing),
                "Snow" => supertypes.push(Supertype::Snow),
                "World" => supertypes.push(Supertype::World),
                "Artifact" => types.push(CardType::Artifact),
                "Battle" => types.push(CardType::Battle),
                "Creature" => types.push(CardType::Creature),
                "Enchantment" => types.push(CardType::Enchantment),
                "Instant" => types.push(CardType::Instant),
                // "Tribal" is the former name of kindred.
                "Kindred" | "Tribal" => types.push(CardType::Kindred),
                "Land" => types.push(CardType::Land),
                "Planeswalker" => types.push(CardType::Planeswalker),
                "Sorcery" => types.push(CardType::Sorcery),
                _ => return Err(err()),
            }
        }
        if types.is_empty() {
            return Err(err());
        }

        Ok(TypeLine {
            supertypes,
            types,
            subtypes: parse_subtypes(right),
        })
    }
}

impl TryFrom<String> for TypeLine {
    type Error = Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<TypeLine> for String {
    fn from(type_line: TypeLine) -> Self {
        type_line.to_string()
    }
}

impl fmt::Display for TypeLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words: Vec<String> = self
            .supertypes
            .iter()
            .map(|s| format!("{s:?}"))
            .chain(self.types.iter().map(|t| format!("{t:?}")))
            .collect();
        write!(f, "{}", words.join(" "))?;
        if !self.subtypes.is_empty() {
            write!(f, " — {}", self.subtypes.join(" "))?;
        }
        Ok(())
    }
}

/// A printed power, toughness, loyalty or defense value (CR 208, 209, 310.4).
///
/// Most are plain numbers; some include a star (CR 208.2) or other
/// non-numeric symbols and are kept verbatim. Serialized as the printed
/// string, e.g. `"3"` or `"1+*"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Stat {
    Fixed(i32),
    Variable(String),
}

impl FromStr for Stat {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        Ok(match s.parse::<i32>() {
            Ok(n) => Stat::Fixed(n),
            Err(_) => Stat::Variable(s.to_string()),
        })
    }
}

impl fmt::Display for Stat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stat::Fixed(n) => write!(f, "{n}"),
            Stat::Variable(s) => write!(f, "{s}"),
        }
    }
}

impl From<String> for Stat {
    fn from(s: String) -> Self {
        let Ok(stat) = s.parse();
        stat
    }
}

impl From<Stat> for String {
    fn from(stat: Stat) -> Self {
        stat.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Mythic,
    Special,
    Bonus,
}

impl FromStr for Rarity {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "common" => Ok(Rarity::Common),
            "uncommon" => Ok(Rarity::Uncommon),
            "rare" => Ok(Rarity::Rare),
            "mythic" => Ok(Rarity::Mythic),
            "special" => Ok(Rarity::Special),
            "bonus" => Ok(Rarity::Bonus),
            _ => Err(Error::Rarity(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use CardType::*;

    fn tl(s: &str) -> TypeLine {
        s.parse().unwrap()
    }

    // CR 205.2a: every traditional card type parses on its own.
    #[test]
    fn every_card_type() {
        for (s, t) in [
            ("Artifact", Artifact),
            ("Battle", Battle),
            ("Creature", Creature),
            ("Enchantment", Enchantment),
            ("Instant", Instant),
            ("Kindred", Kindred),
            ("Land", Land),
            ("Planeswalker", Planeswalker),
            ("Sorcery", Sorcery),
        ] {
            assert_eq!(tl(s).types, vec![t], "{s}");
        }
    }

    // CR 205.4a: the supertypes are basic, legendary, ongoing, snow, and world.
    #[test]
    fn every_supertype() {
        for (s, st) in [
            ("Basic Land", Supertype::Basic),
            ("Legendary Creature", Supertype::Legendary),
            ("Ongoing Enchantment", Supertype::Ongoing),
            ("Snow Land", Supertype::Snow),
            ("World Enchantment", Supertype::World),
        ] {
            assert_eq!(tl(s).supertypes, vec![st], "{s}");
        }
    }

    // CR 205.2b: objects can have more than one card type.
    #[test]
    fn multiple_card_types() {
        assert_eq!(
            tl("Artifact Creature — Golem").types,
            vec![Artifact, Creature]
        );
        assert_eq!(tl("Kindred Instant — Elf").types, vec![Kindred, Instant]);
        assert_eq!(
            tl("Enchantment Creature — God").types,
            vec![Enchantment, Creature]
        );
    }

    // CR 205.4a, 205.3b: supertypes precede card types; subtypes follow a long dash.
    #[test]
    fn supertypes_types_and_subtypes() {
        let t = tl("Legendary Snow Creature — Elf Druid");
        assert_eq!(t.supertypes, vec![Supertype::Legendary, Supertype::Snow]);
        assert_eq!(t.types, vec![Creature]);
        assert_eq!(t.subtypes, vec!["Elf", "Druid"]);
    }

    // CR 205.3a: subtypes are optional.
    #[test]
    fn no_subtypes() {
        let t = tl("Sorcery");
        assert!(t.supertypes.is_empty());
        assert!(t.subtypes.is_empty());
    }

    // CR 205.3i: basic land types.
    #[test]
    fn basic_land() {
        let t = tl("Basic Land — Forest");
        assert!(t.has_supertype(Supertype::Basic));
        assert!(t.is(Land));
        assert!(t.has_subtype("Forest"));
    }

    // CR 205.3m: "Time Lord" is a single two-word creature type.
    #[test]
    fn two_word_creature_type() {
        assert_eq!(
            tl("Legendary Creature — Time Lord Doctor").subtypes,
            vec!["Time Lord", "Doctor"]
        );
        assert_eq!(
            tl("Creature — Human Time Lord").subtypes,
            vec!["Human", "Time Lord"]
        );
    }

    // CR 205.3g, 205.3k, 205.3q: subtypes of other card types.
    #[test]
    fn other_subtypes() {
        assert!(tl("Artifact — Equipment").has_subtype("Equipment"));
        assert!(tl("Enchantment — Aura").has_subtype("Aura"));
        assert!(tl("Instant — Arcane").has_subtype("Arcane"));
        assert!(tl("Battle — Siege").has_subtype("Siege"));
        assert!(tl("Legendary Planeswalker — Jace").has_subtype("Jace"));
    }

    // "Tribal" was renamed to "kindred".
    #[test]
    fn tribal_is_kindred() {
        assert_eq!(tl("Tribal Sorcery — Goblin").types, vec![Kindred, Sorcery]);
    }

    #[test]
    fn invalid_type_lines() {
        for bad in [
            "",
            "Legendary",
            "Creature Elf",
            "Plane — Dominaria",
            "Scheme",
        ] {
            assert!(bad.parse::<TypeLine>().is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn spell_detection() {
        assert!(tl("Instant").is_instant_or_sorcery());
        assert!(tl("Kindred Sorcery — Elf").is_instant_or_sorcery());
        assert!(!tl("Creature — Bear").is_instant_or_sorcery());
    }

    // CR 208.1: power and toughness are numbers; 0 and negatives are allowed.
    #[test]
    fn fixed_stats() {
        assert_eq!("0".parse::<Stat>().unwrap(), Stat::Fixed(0));
        assert_eq!("12".parse::<Stat>().unwrap(), Stat::Fixed(12));
        assert_eq!("-1".parse::<Stat>().unwrap(), Stat::Fixed(-1));
    }

    // CR 208.2: some power/toughness values include a star.
    #[test]
    fn variable_stats() {
        for s in ["*", "1+*", "*+1", "*²", "X"] {
            assert_eq!(s.parse::<Stat>().unwrap(), Stat::Variable(s.to_string()));
        }
    }

    #[test]
    fn rarities() {
        assert_eq!("mythic".parse::<Rarity>().unwrap(), Rarity::Mythic);
        assert!("legendary".parse::<Rarity>().is_err());
    }
}
