---
name: download-set
description: Download Magic: The Gathering card data for a specific set from Scryfall into data/<code>/ (one TOML file per card), or list the sets available to download. Use when the user asks to download, fetch, update or refresh a set's cards, or asks which sets/set codes are available.
---

# Download a set / list sets

This wraps the `download-database` CLI. Full reference: `docs/download.md`.
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
