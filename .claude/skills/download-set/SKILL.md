---
name: download-set
description: Download Magic: The Gathering card data for a specific set from Scryfall into data/<code>/ (one TOML file per card), or list the sets available to download. Use when the user asks to download, fetch, update or refresh a set's cards, or asks which sets/set codes are available.
---

# Download a set / list sets

This wraps the `download-database` CLI, which downloads cards from the
[Scryfall API](https://scryfall.com/docs/api) and saves each set under `data/`
(gitignored), one TOML file per card.
Run every command from the repository root. Don't pipe the output into `head`
or similar, because the program panics on a broken pipe. Read the full output
instead.

## List available sets

1. Run `cargo run -q -- sets --json`. Add `--all` only if the user asks for
   every set, or for tokens, promos or digital sets.
2. If the user named a filter (a year, a type such as "commander", a word in
   the name, "latest N"), apply it to the JSON. With no filter, show the 15
   newest sets and say how many more there are.
3. Reply with **exactly** this format: a markdown table, newest first, then
   one line with the total.

   ```
   | Code | Name | Released | Type | Cards |
   | --- | --- | --- | --- | --- |
   | `fra` | Reality Fracture | 2026-10-02 | expansion | 461 |
   | `frc` | Reality Fracture Commander | 2026-10-02 | commander | 103 |

   2 sets · Cards counts every printing, including alternate arts. Download with "download <code>".
   ```

   - The code is in backticks and is lowercase.
   - `Released` is `YYYY-MM-DD`. Mark a future date with ` (upcoming)`, using
     today's date to decide.
   - `Type` is Scryfall's `set_type` exactly as given.

## Download a set

1. **Resolve the code.** If the user gave a code (e.g. `fra`), use it. If
   they gave a name ("the Hobbit set", "Reality Fracture"), run
   `cargo run -q -- sets --all --json` and match it against `name`
   case-insensitively:
   - One match: use its code.
   - Several matches, e.g. the main set and its Commander or token sets:
     choose the `expansion`/`core` set unless the user said otherwise, and
     name the others in your reply.
   - No match: show the closest names in the table format above and ask
     which one they meant. Do not guess.
2. Run `cargo run -q -- download <code> [<code>...]`.
3. Reply in this format:

   ```
   **<Set name> (`<code>`)**: <parsed> cards parsed, <skipped> skipped, of <total> on Scryfall → `data/<code>/`

   Skipped (<skipped>):
   - <reason>: <count> (e.g. `unsupported card layout: prepare`: 21)
   ```

   Group the skipped cards by reason and leave the list out if nothing was
   skipped. List the card names only if the user asks for them or there are
   5 or fewer.

## Errors

- `invalid set code`: the code must be letters or digits. Resolve it by name
  instead (step 1).
- `no cards found for set`: the code doesn't exist, or the set has no cards
  yet. Suggest close matches from `sets --all --json`.
- `http request failed`: report it to the user. Retry once at most. Scryfall
  rate-limits heavy use.
- When several codes are given, the sets before the failing one are already
  written.

## What a download does

- `download` keeps one printing per card, so it reports fewer cards than the
  `card_count` in `sets`, which counts every printing (alternate arts,
  showcases).
- Re-downloading replaces the set: every `.toml` file in `data/<code>/` is
  deleted first. Other files there are kept.
- Cards are **skipped**, not fatal, when the model can't represent them yet:
  layouts other than `normal`, `transform` and `modal_dfc` (meld, split, flip,
  adventure, `prepare`, `saga`, …), and malformed cards such as a creature face
  without power/toughness.

## Card files

`data/<code>/<collector number>-<name slug>.toml`, with the number
zero-padded to three digits so files sort in collector order, e.g.
`data/fra/001-emrakul-the-exigent-doom.toml`. The fields mirror `Card` in
`src/card/mod.rs`:

- `mana_cost` is as printed, e.g. `"{2}{W/U}"`. `""` means no mana cost
  (lands), which is different from `"{0}"`.
- `colors` and `color_identity` use lowercase names: `white`, `blue`, `black`,
  `red`, `green`.
- `power`, `toughness`, `loyalty` and `defense` are strings, because they can be
  `"*"` or `"1+*"`. They're left out when the card doesn't have them, as is
  `image_url` when Scryfall has no image.
- `[[abilities]]` has one table per ability. `kind` is `keyword`, `triggered`,
  `activated`, `static` or `spell`. Keywords also have `keyword` (the rules
  name, e.g. `"First Strike"`), and activated abilities have `cost` and
  `effect`. The rest have `text`.
- **Double-faced cards** are one file named after the front face. The top-level
  fields are the front face, which is all the card has outside the game
  (CR 712.8a). A `[back_face]` table holds the other side, with the same fields
  plus `kind`, which is `nonmodal` (transforms, CR 712.2) or `modal` (either
  face can be cast or played, CR 712.3). A nonmodal back face has the front
  face's `mana_value` (CR 202.3b), and `color_identity` includes the back face
  (CR 903.4d).
