//! Saving sets to disk: one directory per set, one TOML file per card.
//!
//! ```text
//! data/fra/
//!   001-emrakul-the-exigent-doom.toml
//!   002-...
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use crate::card::Card;
use crate::error::Error;

/// Directory a set is saved to under `root`, e.g. `data/fra`.
pub fn set_dir(root: &Path, code: &str) -> PathBuf {
    root.join(code.to_ascii_lowercase())
}

/// File name for a card: zero-padded collector number and a slug of the
/// name, so files sort in collector order, e.g. `001-emrakul-the-exigent-doom.toml`.
pub fn card_file_name(card: &Card) -> String {
    let number = &card.collector_number;
    let digits = number.len()
        - number
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .len();
    let pad = if digits == 0 {
        0
    } else {
        3usize.saturating_sub(digits)
    };
    let padded = format!("{}{number}", "0".repeat(pad));
    format!("{}-{}.toml", slug(&padded), slug(&card.name))
}

/// Lowercase ASCII letters and digits, with every other run of characters
/// collapsed to a single `-`.
pub(crate) fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Writes every card to its own file in `dir`, creating it if needed.
///
/// Existing `.toml` files in `dir` are removed first, so cards that are no
/// longer in the set don't linger. Other files are left alone.
pub fn write_set(dir: &Path, cards: &[Card]) -> Result<(), Error> {
    fs::create_dir_all(dir)?;
    for path in toml_files(dir)? {
        fs::remove_file(path)?;
    }
    for card in cards {
        fs::write(dir.join(card_file_name(card)), toml::to_string(card)?)?;
    }
    Ok(())
}

/// Reads every card file in `dir`, in file name (collector number) order.
pub fn read_set(dir: &Path) -> Result<Vec<Card>, Error> {
    toml_files(dir)?
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path)?;
            toml::from_str(&text).map_err(|source| Error::CardFile { path, source })
        })
        .collect()
}

fn toml_files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "toml") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::testing::card;
    use crate::card::{Ability, Keyword};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("magym-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn file_names_sort_by_collector_number() {
        let name = |n| card_file_name(&card("Lim-Dûl's Vault", n, "{U}{B}", "Instant", ""));
        assert_eq!(name("7"), "007-lim-d-l-s-vault.toml");
        assert_eq!(name("123a"), "123a-lim-d-l-s-vault.toml");
        assert_eq!(name("1234"), "1234-lim-d-l-s-vault.toml");
        assert_eq!(name("★5"), "5-lim-d-l-s-vault.toml");
    }

    #[test]
    fn toml_shape() {
        let c = card(
            "Grizzled Tester",
            "12",
            "{1}{G/W}",
            "Legendary Creature — Elf Druid",
            "Flying, first strike\nWhen this creature enters, draw a card.\n{T}: Add {G}.",
        );
        let text = toml::to_string(&c).unwrap();
        for line in [
            r#"mana_cost = "{1}{G/W}""#,
            r#"type_line = "Legendary Creature — Elf Druid""#,
            r#"colors = ["white", "green"]"#,
            r#"power = "2""#,
            r#"toughness = "1+*""#,
            r#"rarity = "uncommon""#,
            r#"kind = "keyword""#,
            r#"keyword = "First Strike""#,
            r#"kind = "triggered""#,
            r#"kind = "activated""#,
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "missing {line:?} in:\n{text}"
            );
        }
        // Rules text keeps its line breaks instead of `\n` escapes.
        assert!(
            text.contains("oracle_text = \"\"\"\nFlying, first strike\n"),
            "{text}"
        );
        // Absent characteristics are left out rather than written as empty.
        assert!(!text.contains("loyalty"), "{text}");
        assert!(!text.contains("image_url"), "{text}");
        assert_eq!(toml::from_str::<Card>(&text).unwrap(), c);
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = temp_dir("roundtrip");
        let cards = vec![
            card("Second", "2", "{2}{R}", "Sorcery", "Draw two cards."),
            card("First", "1", "", "Land", "{T}: Add {C}."),
        ];
        write_set(&dir, &cards).unwrap();
        let read = read_set(&dir).unwrap();
        assert_eq!(read, vec![cards[1].clone(), cards[0].clone()]);
        // A land's empty cost survives and stays distinct from {0} (CR 202.1b).
        assert!(read[0].mana_cost.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rewriting_removes_stale_cards_only() {
        let dir = temp_dir("rewrite");
        write_set(&dir, &[card("Old", "1", "{W}", "Instant", "")]).unwrap();
        fs::write(dir.join("notes.txt"), "keep me").unwrap();
        write_set(&dir, &[card("New", "2", "{W}", "Instant", "")]).unwrap();

        let names: Vec<String> = read_set(&dir)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["New"]);
        assert!(dir.join("notes.txt").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_keywords_keep_their_name() {
        let c = card("Tester", "1", "{B}", "Creature — Human", "Flashback {2}{B}");
        let back: Card = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert!(matches!(
            &back.abilities[0],
            Ability::Keyword { keyword: Keyword::Other(name), .. } if name == "Flashback"
        ));
    }

    #[test]
    fn bad_file_names_its_path() {
        let dir = temp_dir("bad");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("001-broken.toml"), "name = 3").unwrap();
        let err = read_set(&dir).unwrap_err().to_string();
        assert!(err.contains("001-broken.toml"), "{err}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
