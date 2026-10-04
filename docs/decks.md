# Decks

A [`Deck`](../src/deck/mod.rs) is a name, a format, a main deck, a sideboard
and, for Commander, a commander. It refers to cards by printing (set and
collector number) rather than copying them, so it needs the card data in
`data/` (see [download.md](download.md)) to be validated.

## Building

```rust
use download_database::deck::{Deck, Format};
use download_database::pool::CardPool;

let pool = CardPool::load(Path::new("data"))?;   // every downloaded set
let bolt = pool.named("Lightning Bolt").next().unwrap();

let mut deck = Deck::new("Mono-Red Burn", Format::Constructed);
deck.add(bolt, 4);
deck.remove(bolt, 1);                           // returns how many were removed

for violation in deck.validate(&pool) {         // empty = legal
    println!("{violation}");
}
```

Building never fails: a deck is usually illegal until it's finished.
`validate` reports every problem at once, following the Comprehensive Rules:

| Format | Rules checked |
| --- | --- |
| `constructed` | ≥ 60 cards (100.2a); ≤ 4 per English name across deck and sideboard, basic lands excepted (100.2a, 100.4a); sideboard ≤ 15 (100.4a) |
| `limited` | ≥ 40 cards (100.2b); any sideboard size (100.4b) |
| `commander` | a valid commander (903.3, 903.3a); exactly 100 cards including it (903.5a); one per English name, basic lands excepted (903.5b); color identity (903.5c, 903.5d); no sideboard (903.5e) |

Cards like Relentless Rats ("A deck can have any number of cards named …")
override the copy limit (113.6n). Not checked yet: interchangeable names
(201.3b), Limited's copies-per-product limit (100.2b), and format
legality lists (banned/restricted, Standard rotation), which come from the
Tournament Rules rather than the Comprehensive Rules.

## Saving

```rust
use download_database::deck::{DeckStore, TomlDeckStore};

let store = TomlDeckStore::new("decks");
store.save(&deck)?;                       // decks/mono-red-burn.toml
let deck = store.load("Mono-Red Burn")?;
store.list()?;                            // ["Mono-Red Burn", ...]
store.delete("Mono-Red Burn")?;           // true if it existed
```

`DeckStore` is a trait, so another backend can replace the TOML files later.
`decks/` is gitignored, like `data/`.

```toml
name = "Mono-Red Burn"
format = "constructed"
description = "Go face."

[[main]]
count = 20
name = "Mountain"
set = "fra"
collector_number = "271"

[[main]]
count = 4
name = "Lightning Bolt"
set = "fra"
collector_number = "142"

[[sideboard]]
count = 2
name = "Pyroblast"
set = "fra"
collector_number = "150"
```

A Commander deck has a `[commander]` table with `name`, `set` and
`collector_number`, and no `[[sideboard]]`.
