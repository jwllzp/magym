# Agent guidelines

## Game rules

The Magic: The Gathering Comprehensive Rules are the source of truth for game behavior.
They are split into small files under `docs/rules/` — start at `docs/rules/README.md`
for the index, and read only the files relevant to the task (grep by rule number or term)
rather than loading whole chapters.

`docs/rules/` is generated and gitignored: the rules are copyrighted and
can't be redistributed in this repo. If it's missing, follow the `setup` skill
(`.claude/skills/setup/SKILL.md`).

## Testing

- Unit tests must be based on the official Comprehensive Rules, not on the current
  implementation. If the code and the rules disagree, the test should fail.
- Reference the rule each test verifies, in the test name or a comment
  (e.g. `// CR 704.5a: a player with 0 or less life loses the game`).
- If the rules are ambiguous or don't cover a case, ask instead of guessing.

## Updating the rules

Don't edit `docs/rules/` by hand, and never commit it. When a new rules version is
published, rerun the `setup` skill with its URL.
