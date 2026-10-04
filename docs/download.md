# Downloading card data

magym gets its card data from the [Scryfall API](https://scryfall.com/docs/api).
The `download-database` crate downloads a set, converts every card into the
typed [`Card`](../src/card/mod.rs) model, and saves each set as a directory
of TOML files, one per card, under `data/` (which is gitignored).

## Command line

```sh
cargo run -q -- sets                 # list playable paper sets, newest first
cargo run -q -- sets --all           # every set, including tokens, promos, digital
cargo run -q -- sets --json          # same list as JSON (combine with --all)
cargo run -q -- download fra         # download one set to data/fra/
cargo run -q -- download fra hob     # download several sets
cargo run -q -- download             # no code: Reality Fracture (fra)
```

### `sets`

Prints one set per line:

```
CODE    RELEASED    TYPE              CARDS  NAME
fra     2026-10-02  expansion           461  Reality Fracture
frc     2026-10-02  commander           103  Reality Fracture Commander
```

- `CODE` is what you pass to `download`.
- `RELEASED` can be in the future for announced sets; their card lists may be
  incomplete.
- `CARDS` is Scryfall's count of **every printing** in the set, including
  alternate arts and showcase versions. `download` keeps one printing per card,
  so it usually reports fewer cards.
- Without `--all`, only non-digital sets of type `core`, `expansion`,
  `masters`, `draft_innovation` and `commander` are listed
  (`scryfall::PLAYABLE_SET_TYPES`).

`--json` prints an array of objects with the fields `code`, `name`,
`set_type`, `released_at`, `card_count` and `digital`.

### `download`

For each code, prints a summary, the cards it skipped, and the output file:

```
fra: 264 cards parsed, 21 skipped (of 285 on Scryfall)
  skipped Blossom-Blessed Angel // Seed Suture: unsupported card layout: prepare
  ...
wrote 264 files to data/fra/
```

Running it again replaces the set: every `.toml` file in `data/<code>/` is
deleted before the new ones are written, so cards Scryfall no longer lists
don't linger. Other files in the directory are kept.

## Card files

```
data/fra/
  001-emrakul-the-exigent-doom.toml
  002-academic-ascent.toml
  ...
```

Each file is named `<collector number>-<name slug>.toml`. The number is
zero-padded to three digits so files sort in collector order.

```toml
name = "Emrakul, the Exigent Doom"
mana_cost = "{10}"
mana_value = 10
type_line = "Legendary Creature — Eldrazi"
colors = []
color_identity = []
power = "12"
toughness = "12"
rarity = "mythic"
set = "fra"
collector_number = "1"
id = "c3ff8dd3-88a8-49dc-a59b-e2748680623c"
oracle_id = "4421ab7d-6d9b-4edd-b5a0-53a8ed84da6f"
image_url = "https://cards.scryfall.io/normal/front/c/3/c3ff8dd3-....jpg"
oracle_text = """
When you cast this spell, untap all lands you control.
Flying, trample
Ward—Sacrifice three permanents.
{3}, Exile this card from your hand: Target land gains "{T}: Add {C}{C}" until ..."""

[[abilities]]
kind = "triggered"
text = "When you cast this spell, untap all lands you control."

[[abilities]]
kind = "keyword"
keyword = "Flying"
text = "Flying"

[[abilities]]
kind = "activated"
cost = "{3}, Exile this card from your hand"
effect = 'Target land gains "{T}: Add {C}{C}" until ...'
```

| Field | Format |
| --- | --- |
| `mana_cost` | As printed, e.g. `"{2}{W/U}"`. `""` means no mana cost (lands), which is different from `"{0}"`. |
| `type_line` | As printed, e.g. `"Legendary Creature — Elf Druid"`. |
| `colors`, `color_identity` | Lowercase names: `white`, `blue`, `black`, `red`, `green`. |
| `power`, `toughness`, `loyalty`, `defense` | Strings, because they can be `"*"` or `"1+*"`. Left out when the card doesn't have them. |
| `rarity` | `common`, `uncommon`, `rare`, `mythic`, `special` or `bonus`. |
| `image_url` | Left out when Scryfall has no image. |
| `abilities` | One `[[abilities]]` table per ability. `kind` is `keyword`, `triggered`, `activated`, `static` or `spell`. Keywords also have `keyword` (the rules name, e.g. `"First Strike"`) and `text` (as printed). Activated abilities have `cost` and `effect`; the rest have `text`. |

Cards are **skipped**, not fatal, when they can't be represented by the model
yet. The usual reason is a layout other than `normal`, such as two-faced
cards (transform, modal DFC, split, adventure), set mechanics like `prepare`,
and sagas, which Scryfall gives their own `saga` layout. Cards with malformed data are skipped too, for example a creature
with no power/toughness.

Errors stop the command with exit code 1:

| Message | Cause |
| --- | --- |
| `invalid set code: "…"` | The code is not 1–8 ASCII letters/digits. |
| `no cards found for set "…"` | Scryfall has no cards for that code. Check `sets --all`. |
| `http request failed: …` | Network problem or a Scryfall outage. |

When several codes are given, the sets before the failing one are already
written.

## Library

```rust
use download_database::scryfall;

// Every set, newest first.
let sets = scryfall::list_sets()?;
let playable: Vec<_> = sets.into_iter().filter(|s| s.is_playable()).collect();

// One set.
let report = scryfall::fetch_set("fra")?;  // or scryfall::fetch_reality_fracture()
report.cards;        // Vec<Card>
report.skipped;      // Vec<Skipped { name, reason }>
report.total_cards;  // what Scryfall reported; cards + skipped == total_cards

// Save and load.
use download_database::store;
let dir = store::set_dir(Path::new("data"), "fra");  // data/fra
store::write_set(&dir, &report.cards)?;
let cards = store::read_set(&dir)?;                    // collector order
```

| Function | Returns | Errors |
| --- | --- | --- |
| `list_sets()` | `Vec<SetInfo>` | `Http`, `Json` |
| `fetch_set(code)` | `FetchReport` | `SetCode`, `UnknownSet`, `Http`, `Json` |
| `fetch_reality_fracture()` | `FetchReport` | same as `fetch_set` |
| `store::write_set(dir, cards)` | `()` | `Io`, `TomlWrite` |
| `store::read_set(dir)` | `Vec<Card>` | `Io`, `CardFile` (names the bad file) |

`fetch_set` accepts any capitalization (`FRA` and `fra` are the same set). It
follows Scryfall's pagination and waits 100 ms between requests, as Scryfall
asks.

## Tests

```sh
cargo test                 # offline unit tests
cargo test -- --ignored    # live tests against Scryfall (needs network)
```

## Agent skill

The `download-set` skill (`.claude/skills/download-set/SKILL.md`) lets you ask
an agent things like *"download the Hobbit set"* or *"what sets can I
download?"*. It wraps the commands above.
