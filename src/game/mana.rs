//! Mana in the game: the six types of mana, mana pools, and paying costs.

use serde::{Deserialize, Serialize};

use crate::card::{Color, ManaCost, ManaSymbol};

/// A type of mana (CR 106.1b): one of the five colors, or colorless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mana {
    White,
    Blue,
    Black,
    Red,
    Green,
    Colorless,
}

impl Mana {
    pub const ALL: [Mana; 6] = [
        Mana::White,
        Mana::Blue,
        Mana::Black,
        Mana::Red,
        Mana::Green,
        Mana::Colorless,
    ];

    fn index(self) -> usize {
        self as usize
    }
}

impl From<Color> for Mana {
    fn from(color: Color) -> Self {
        match color {
            Color::White => Mana::White,
            Color::Blue => Mana::Blue,
            Color::Black => Mana::Black,
            Color::Red => Mana::Red,
            Color::Green => Mana::Green,
        }
    }
}

/// The mana that symbols such as `{G}{G}` or `{C}` produce, as written in
/// "Add {G}{G}" (CR 106.1a). `None` if a symbol isn't a single type of mana.
pub fn produced(symbols: &ManaCost) -> Option<Vec<Mana>> {
    symbols
        .0
        .iter()
        .map(|s| match s {
            ManaSymbol::Colored(c) => Some(Mana::from(*c)),
            ManaSymbol::Colorless => Some(Mana::Colorless),
            _ => None,
        })
        .collect()
}

/// A player's mana pool (CR 106.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaPool([u32; 6]);

impl ManaPool {
    pub fn get(&self, mana: Mana) -> u32 {
        self.0[mana.index()]
    }

    pub fn add(&mut self, mana: Mana, amount: u32) {
        self.0[mana.index()] += amount;
    }

    pub fn total(&self) -> u32 {
        self.0.iter().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub fn clear(&mut self) {
        self.0 = [0; 6];
    }

    fn take(&mut self, mana: Mana) -> bool {
        let n = &mut self.0[mana.index()];
        if *n == 0 {
            return false;
        }
        *n -= 1;
        true
    }

    /// Pays `cost` from this pool and returns what's left, or `None` if the
    /// pool can't pay it (CR 601.2h, partial payments aren't allowed).
    ///
    /// Generic mana is paid with colorless mana first, then with the colors
    /// the pool has most of. {X}, snow and Phyrexian symbols can't be paid
    /// yet.
    pub fn pay(&self, cost: &ManaCost) -> Option<ManaPool> {
        let mut pool = *self;
        let mut generic = 0;
        let mut flexible = Vec::new();
        for symbol in &cost.0 {
            match *symbol {
                ManaSymbol::Colored(c) => {
                    if !pool.take(c.into()) {
                        return None;
                    }
                }
                ManaSymbol::Colorless => {
                    if !pool.take(Mana::Colorless) {
                        return None;
                    }
                }
                ManaSymbol::Generic(n) => generic += n,
                ManaSymbol::Hybrid(..) | ManaSymbol::TwoBrid(_) => flexible.push(*symbol),
                ManaSymbol::X
                | ManaSymbol::Snow
                | ManaSymbol::Phyrexian(_)
                | ManaSymbol::PhyrexianHybrid(..) => return None,
            }
        }
        pool.pay_flexible(&flexible, generic)
    }

    /// Hybrid symbols (CR 107.4e) can be paid with either half; try each.
    fn pay_flexible(self, flexible: &[ManaSymbol], generic: u32) -> Option<ManaPool> {
        let Some((first, rest)) = flexible.split_first() else {
            return self.pay_generic(generic);
        };
        match *first {
            ManaSymbol::Hybrid(a, b) => [a, b].into_iter().find_map(|c| {
                let mut pool = self;
                pool.take(c.into())
                    .then(|| pool.pay_flexible(rest, generic))
                    .flatten()
            }),
            ManaSymbol::TwoBrid(c) => {
                let mut pool = self;
                pool.take(c.into())
                    .then(|| pool.pay_flexible(rest, generic))
                    .flatten()
                    .or_else(|| self.pay_flexible(rest, generic + 2))
            }
            _ => unreachable!("only hybrid symbols are flexible"),
        }
    }

    fn pay_generic(mut self, mut generic: u32) -> Option<ManaPool> {
        if self.total() < generic {
            return None;
        }
        while generic > 0 {
            let mana = if self.get(Mana::Colorless) > 0 {
                Mana::Colorless
            } else {
                *Mana::ALL
                    .iter()
                    .max_by_key(|m| (self.get(**m), std::cmp::Reverse(m.index())))
                    .unwrap()
            };
            self.take(mana);
            generic -= 1;
        }
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(mana: &[Mana]) -> ManaPool {
        let mut pool = ManaPool::default();
        for m in mana {
            pool.add(*m, 1);
        }
        pool
    }

    fn cost(s: &str) -> ManaCost {
        s.parse().unwrap()
    }

    use Mana::*;

    // CR 107.4b: colored symbols need mana of that color.
    #[test]
    fn colored_needs_its_color() {
        assert!(pool(&[Red]).pay(&cost("{R}")).is_some());
        assert!(pool(&[Green]).pay(&cost("{R}")).is_none());
    }

    // CR 107.4c: {C} can be paid only with colorless mana.
    #[test]
    fn colorless_symbol_needs_colorless_mana() {
        assert!(pool(&[Colorless]).pay(&cost("{C}")).is_some());
        assert!(pool(&[Red]).pay(&cost("{C}")).is_none());
    }

    // CR 107.4b: generic mana can be paid with any type of mana.
    #[test]
    fn generic_takes_any_mana() {
        let left = pool(&[Red, Green, Green]).pay(&cost("{1}{G}")).unwrap();
        assert_eq!(left.total(), 1);
        assert!(pool(&[Green]).pay(&cost("{1}{G}")).is_none());
    }

    // CR 107.4e: a hybrid symbol is paid with either of its colors.
    #[test]
    fn hybrid_takes_either_color() {
        assert!(pool(&[White]).pay(&cost("{W/U}")).is_some());
        assert!(pool(&[Blue]).pay(&cost("{W/U}")).is_some());
        assert!(pool(&[Black]).pay(&cost("{W/U}")).is_none());
        // The second hybrid symbol must use the color the first one left.
        assert!(pool(&[White, Blue]).pay(&cost("{W/U}{W}")).is_some());
    }

    // CR 107.4e: {2/W} is paid with one white mana or two of any type.
    #[test]
    fn twobrid_takes_its_color_or_two() {
        assert!(pool(&[White]).pay(&cost("{2/W}")).is_some());
        assert!(pool(&[Red, Red]).pay(&cost("{2/W}")).is_some());
        assert!(pool(&[Red]).pay(&cost("{2/W}")).is_none());
    }

    // CR 202.1b: a {0} cost is paid with no mana.
    #[test]
    fn zero_cost_is_free() {
        assert_eq!(
            ManaPool::default().pay(&cost("{0}")),
            Some(ManaPool::default())
        );
    }

    #[test]
    fn produced_mana() {
        assert_eq!(produced(&cost("{G}{G}")), Some(vec![Green, Green]));
        assert_eq!(produced(&cost("{C}")), Some(vec![Colorless]));
        assert_eq!(produced(&cost("{1}")), None);
    }
}
