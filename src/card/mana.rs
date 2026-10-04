use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The five colors of Magic (CR 105.1), in WUBRG order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    White,
    Blue,
    Black,
    Red,
    Green,
}

impl Color {
    pub const ALL: [Color; 5] = [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ];

    pub fn symbol(self) -> char {
        match self {
            Color::White => 'W',
            Color::Blue => 'U',
            Color::Black => 'B',
            Color::Red => 'R',
            Color::Green => 'G',
        }
    }
}

impl FromStr for Color {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "W" => Ok(Color::White),
            "U" => Ok(Color::Blue),
            "B" => Ok(Color::Black),
            "R" => Ok(Color::Red),
            "G" => Ok(Color::Green),
            _ => Err(Error::Color(s.to_string())),
        }
    }
}

/// A single mana symbol as printed in a mana cost (CR 107.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManaSymbol {
    /// `{0}`, `{1}`, `{2}`, ...
    Generic(u32),
    /// `{W}`, `{U}`, `{B}`, `{R}`, `{G}`
    Colored(Color),
    /// `{C}`
    Colorless,
    /// `{S}`
    Snow,
    /// `{X}`
    X,
    /// `{W/U}`
    Hybrid(Color, Color),
    /// `{2/W}`
    TwoBrid(Color),
    /// `{W/P}`
    Phyrexian(Color),
    /// `{W/U/P}`
    PhyrexianHybrid(Color, Color),
}

impl ManaSymbol {
    /// Contribution of this symbol to a card's mana value (CR 202.3).
    pub fn mana_value(self) -> u32 {
        match self {
            ManaSymbol::Generic(n) => n,
            ManaSymbol::X => 0,
            ManaSymbol::TwoBrid(_) => 2,
            ManaSymbol::Colored(_)
            | ManaSymbol::Colorless
            | ManaSymbol::Snow
            | ManaSymbol::Hybrid(..)
            | ManaSymbol::Phyrexian(_)
            | ManaSymbol::PhyrexianHybrid(..) => 1,
        }
    }

    /// Colors this symbol contributes to a card's color (CR 202.2).
    pub fn colors(self) -> Vec<Color> {
        match self {
            ManaSymbol::Colored(c) | ManaSymbol::TwoBrid(c) | ManaSymbol::Phyrexian(c) => vec![c],
            ManaSymbol::Hybrid(a, b) | ManaSymbol::PhyrexianHybrid(a, b) => vec![a, b],
            ManaSymbol::Generic(_) | ManaSymbol::Colorless | ManaSymbol::Snow | ManaSymbol::X => {
                vec![]
            }
        }
    }

    fn parse(inner: &str) -> Option<Self> {
        if let Ok(n) = inner.parse::<u32>() {
            return Some(ManaSymbol::Generic(n));
        }
        let parts: Vec<&str> = inner.split('/').collect();
        match parts.as_slice() {
            ["C"] => Some(ManaSymbol::Colorless),
            ["S"] => Some(ManaSymbol::Snow),
            ["X"] => Some(ManaSymbol::X),
            [c] => c.parse().ok().map(ManaSymbol::Colored),
            ["2", c] => c.parse().ok().map(ManaSymbol::TwoBrid),
            [c, "P"] => c.parse().ok().map(ManaSymbol::Phyrexian),
            [a, b] => Some(ManaSymbol::Hybrid(a.parse().ok()?, b.parse().ok()?)),
            [a, b, "P"] => Some(ManaSymbol::PhyrexianHybrid(
                a.parse().ok()?,
                b.parse().ok()?,
            )),
            _ => None,
        }
    }
}

impl fmt::Display for ManaSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManaSymbol::Generic(n) => write!(f, "{{{n}}}"),
            ManaSymbol::Colored(c) => write!(f, "{{{}}}", c.symbol()),
            ManaSymbol::Colorless => write!(f, "{{C}}"),
            ManaSymbol::Snow => write!(f, "{{S}}"),
            ManaSymbol::X => write!(f, "{{X}}"),
            ManaSymbol::Hybrid(a, b) => write!(f, "{{{}/{}}}", a.symbol(), b.symbol()),
            ManaSymbol::TwoBrid(c) => write!(f, "{{2/{}}}", c.symbol()),
            ManaSymbol::Phyrexian(c) => write!(f, "{{{}/P}}", c.symbol()),
            ManaSymbol::PhyrexianHybrid(a, b) => {
                write!(f, "{{{}/{}/P}}", a.symbol(), b.symbol())
            }
        }
    }
}

/// A card's mana cost, e.g. `{2}{W/U}{X}` (CR 202.1).
///
/// An empty cost (lands, CR 202.1b) is distinct from `{0}`. Serialized as
/// the printed string, e.g. `"{2}{W/U}"`, and `""` for no cost.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ManaCost(pub Vec<ManaSymbol>);

impl ManaCost {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Mana value (CR 202.3).
    pub fn mana_value(&self) -> u32 {
        self.0.iter().map(|s| s.mana_value()).sum()
    }

    /// Colors indicated by the cost (CR 202.2), deduplicated in WUBRG order.
    pub fn colors(&self) -> Vec<Color> {
        let mut colors: Vec<Color> = self.0.iter().flat_map(|s| s.colors()).collect();
        colors.sort();
        colors.dedup();
        colors
    }
}

impl FromStr for ManaCost {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || Error::ManaCost(s.to_string());
        let mut symbols = Vec::new();
        let mut rest = s.trim();
        while !rest.is_empty() {
            let inner_and_tail = rest.strip_prefix('{').ok_or_else(err)?;
            let end = inner_and_tail.find('}').ok_or_else(err)?;
            symbols.push(ManaSymbol::parse(&inner_and_tail[..end]).ok_or_else(err)?);
            rest = &inner_and_tail[end + 1..];
        }
        Ok(ManaCost(symbols))
    }
}

impl fmt::Display for ManaCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|s| write!(f, "{s}"))
    }
}

impl TryFrom<String> for ManaCost {
    type Error = Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<ManaCost> for String {
    fn from(cost: ManaCost) -> Self {
        cost.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Color::*;
    use ManaSymbol::*;

    fn cost(s: &str) -> ManaCost {
        s.parse().unwrap()
    }

    // CR 107.4b: generic mana symbols {0}, {1}, {2}, ... {X}.
    #[test]
    fn generic_symbols() {
        for n in 0..=16 {
            assert_eq!(cost(&format!("{{{n}}}")).0, vec![Generic(n)]);
        }
    }

    // CR 107.4a: the five primary colored mana symbols.
    #[test]
    fn colored_symbols() {
        assert_eq!(
            cost("{W}{U}{B}{R}{G}").0,
            vec![
                Colored(White),
                Colored(Blue),
                Colored(Black),
                Colored(Red),
                Colored(Green)
            ]
        );
    }

    // CR 107.4c: the colorless mana symbol {C}.
    #[test]
    fn colorless_symbol() {
        assert_eq!(cost("{C}{C}").0, vec![Colorless, Colorless]);
    }

    // CR 107.4h: the snow mana symbol {S}.
    #[test]
    fn snow_symbol() {
        assert_eq!(cost("{S}").0, vec![Snow]);
    }

    // CR 107.3: X as a placeholder in costs, possibly repeated.
    #[test]
    fn x_symbols() {
        assert_eq!(cost("{X}{X}{R}").0, vec![X, X, Colored(Red)]);
    }

    // CR 107.4e: the ten two-color hybrid symbols.
    #[test]
    fn all_hybrid_pairs() {
        for (i, a) in Color::ALL.iter().enumerate() {
            for b in &Color::ALL[i + 1..] {
                let s = format!("{{{}/{}}}", a.symbol(), b.symbol());
                assert_eq!(cost(&s).0, vec![Hybrid(*a, *b)], "{s}");
            }
        }
    }

    // CR 107.4e: monocolored hybrid ("twobrid") symbols.
    #[test]
    fn twobrid_symbols() {
        for c in Color::ALL {
            assert_eq!(cost(&format!("{{2/{}}}", c.symbol())).0, vec![TwoBrid(c)]);
        }
    }

    // CR 107.4f: Phyrexian mana symbols, including hybrid Phyrexian.
    #[test]
    fn phyrexian_symbols() {
        assert_eq!(cost("{W/P}").0, vec![Phyrexian(White)]);
        assert_eq!(cost("{G/U/P}").0, vec![PhyrexianHybrid(Green, Blue)]);
    }

    // CR 202.1b: some objects have no mana cost; that is not the same as {0}.
    #[test]
    fn empty_cost_differs_from_zero() {
        let none = cost("");
        let zero = cost("{0}");
        assert!(none.is_empty());
        assert!(!zero.is_empty());
        assert_ne!(none, zero);
        assert_eq!(none.mana_value(), 0);
        assert_eq!(zero.mana_value(), 0);
    }

    #[test]
    fn malformed_costs_are_rejected() {
        for bad in ["{W", "W}", "{Q}", "{}", "{W/Q}", "2", "{2}x", "{W/U/B}"] {
            assert!(bad.parse::<ManaCost>().is_err(), "{bad} should fail");
        }
    }

    // CR 202.3: mana value is the total amount of mana in the cost.
    #[test]
    fn mana_value_basic() {
        assert_eq!(cost("{3}{G}{G}").mana_value(), 5);
        assert_eq!(cost("{C}{S}").mana_value(), 2);
    }

    // CR 202.3e: X is treated as 0 outside the stack.
    #[test]
    fn mana_value_x_is_zero() {
        assert_eq!(cost("{X}{X}{R}").mana_value(), 1);
    }

    // CR 202.3f: hybrid symbols count their largest component.
    #[test]
    fn mana_value_hybrid() {
        assert_eq!(cost("{W/U}{W/U}").mana_value(), 2);
        assert_eq!(cost("{2/W}{2/W}{2/W}").mana_value(), 6);
    }

    // CR 202.3g: each Phyrexian symbol counts as 1.
    #[test]
    fn mana_value_phyrexian() {
        assert_eq!(cost("{3}{B/P}").mana_value(), 4);
        assert_eq!(cost("{G/U/P}").mana_value(), 1);
    }

    // CR 202.2: an object is the color(s) of the mana symbols in its cost.
    #[test]
    fn colors_from_cost() {
        assert_eq!(cost("{1}{R}{R}").colors(), vec![Red]);
        assert_eq!(cost("{G}{W}").colors(), vec![White, Green]);
        assert_eq!(cost("{2/B}").colors(), vec![Black]);
        assert_eq!(cost("{U/P}").colors(), vec![Blue]);
        assert_eq!(cost("{R/G}").colors(), vec![Red, Green]);
    }

    // CR 105.2c, 202.2: generic and {C} costs leave an object colorless.
    #[test]
    fn colorless_costs() {
        assert!(cost("{4}").colors().is_empty());
        assert!(cost("{X}{C}{C}").colors().is_empty());
        assert!(cost("").colors().is_empty());
    }

    #[test]
    fn display_round_trips() {
        for s in ["{2}{W/U}{X}", "{2/R}{G/P}{B/G/P}{C}{S}", ""] {
            assert_eq!(cost(s).to_string(), s);
        }
    }
}
