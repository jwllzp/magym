# magym
An agentic tool to help bussy adult minds to get back to a game they love.

## Pilars
1. **Deck building**: specify the kind of deck and format you want to play, an agent will build it for you.
2. **Training**: This will be a [Kata](https://en.wikipedia.org/wiki/Kata) system meant to get you familiarized with your new deck. Will include:
  - Combos.
  - Strategies.
  - First hand picks.
  - Sideboarding.
Think of this section as prototyping a new deck by yourslef in a table.
3. **Puzzles**: Custom made puzzles that will make you think past the basic combos and start connecting the dots in a real world setting.

## Setup

After cloning, run the `setup` skill (`/setup` in Claude Code). It asks for a URL to the
plain-text (`.txt`) Magic: The Gathering Comprehensive Rules, downloads them, and splits
them into small files under `docs/rules/`. That directory is gitignored, because the rules
are copyrighted and aren't redistributed here.

## Game engine

`src/game/` plays two-player games by the Comprehensive Rules: turn structure, priority and
the stack, combat, state-based actions, and triggered and activated abilities. The engine
stops whenever a player has to decide something; callers ask for the pending decision and
its legal actions, then apply one.

Cards are data. Evergreen keywords (flying, trample, deathtouch, …) are built in; anything
else a card does is described by a script in `scripts/`, one TOML file per card, written
with the effect DSL documented in `src/game/effect.rs` and `src/game/script.rs`:

```toml
# scripts/last-gasp.toml
name = "Last Gasp"

[spell]
effect = "pump"
target = { kind = "creature" }
power = -3
toughness = -3
```

```sh
cargo run -- playable fra                       # which downloaded cards the engine can play
cargo run -- play DECK DECK --seed 42           # a game between two random players
```
