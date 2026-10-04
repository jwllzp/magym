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
