use serde::{Deserialize, Serialize};

/// Keyword abilities (CR 702), as listed in the Comprehensive Rules of 2026-09-25.
/// "Daybound and Nightbound" is split in two; typecycling and landwalk variants
/// are matched separately (see [`match_keyword`]).
const KEYWORD_ABILITIES: &[&str] = &[
    "Deathtouch",
    "Defender",
    "Double Strike",
    "Enchant",
    "Equip",
    "First Strike",
    "Flash",
    "Flying",
    "Haste",
    "Hexproof",
    "Indestructible",
    "Intimidate",
    "Landwalk",
    "Lifelink",
    "Protection",
    "Reach",
    "Shroud",
    "Trample",
    "Vigilance",
    "Ward",
    "Banding",
    "Rampage",
    "Cumulative Upkeep",
    "Flanking",
    "Phasing",
    "Buyback",
    "Shadow",
    "Cycling",
    "Echo",
    "Horsemanship",
    "Fading",
    "Kicker",
    "Flashback",
    "Madness",
    "Fear",
    "Morph",
    "Amplify",
    "Provoke",
    "Storm",
    "Affinity",
    "Entwine",
    "Modular",
    "Sunburst",
    "Bushido",
    "Soulshift",
    "Splice",
    "Offering",
    "Ninjutsu",
    "Epic",
    "Convoke",
    "Dredge",
    "Transmute",
    "Bloodthirst",
    "Haunt",
    "Replicate",
    "Forecast",
    "Graft",
    "Recover",
    "Ripple",
    "Split Second",
    "Suspend",
    "Vanishing",
    "Absorb",
    "Aura Swap",
    "Delve",
    "Fortify",
    "Frenzy",
    "Gravestorm",
    "Poisonous",
    "Transfigure",
    "Champion",
    "Changeling",
    "Evoke",
    "Hideaway",
    "Prowl",
    "Reinforce",
    "Conspire",
    "Persist",
    "Wither",
    "Retrace",
    "Devour",
    "Exalted",
    "Unearth",
    "Cascade",
    "Annihilator",
    "Level Up",
    "Rebound",
    "Umbra Armor",
    "Infect",
    "Battle Cry",
    "Living Weapon",
    "Undying",
    "Miracle",
    "Soulbond",
    "Overload",
    "Scavenge",
    "Unleash",
    "Cipher",
    "Evolve",
    "Extort",
    "Fuse",
    "Bestow",
    "Tribute",
    "Dethrone",
    "Hidden Agenda",
    "Outlast",
    "Prowess",
    "Dash",
    "Exploit",
    "Menace",
    "Renown",
    "Awaken",
    "Devoid",
    "Ingest",
    "Myriad",
    "Surge",
    "Skulk",
    "Emerge",
    "Escalate",
    "Melee",
    "Crew",
    "Fabricate",
    "Partner",
    "Undaunted",
    "Improvise",
    "Aftermath",
    "Embalm",
    "Eternalize",
    "Afflict",
    "Ascend",
    "Assist",
    "Jump-Start",
    "Mentor",
    "Afterlife",
    "Riot",
    "Spectacle",
    "Escape",
    "Companion",
    "Mutate",
    "Encore",
    "Boast",
    "Foretell",
    "Demonstrate",
    "Daybound",
    "Nightbound",
    "Disturb",
    "Decayed",
    "Cleave",
    "Training",
    "Compleated",
    "Reconfigure",
    "Blitz",
    "Casualty",
    "Enlist",
    "Read Ahead",
    "Ravenous",
    "Squad",
    "Space Sculptor",
    "Visit",
    "Prototype",
    "Living Metal",
    "More Than Meets the Eye",
    "For Mirrodin!",
    "Toxic",
    "Backup",
    "Bargain",
    "Craft",
    "Disguise",
    "Solved",
    "Plot",
    "Saddle",
    "Spree",
    "Freerunning",
    "Gift",
    "Offspring",
    "Impending",
    "Exhaust",
    "Max Speed",
    "Start Your Engines!",
    "Harmonize",
    "Mobilize",
    "Job Select",
    "Tiered",
    "Station",
    "Warp",
    "Mayhem",
    "Web-slinging",
    "Firebending",
    "Sneak",
    "Increment",
    "Paradigm",
    "Power-up",
    "Teamwork",
    "Storied",
];

/// The keyword abilities a deck builder cares about most, plus a catch-all.
/// Serialized as its rules name, e.g. `"First Strike"` or `"Flashback"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Keyword {
    Cycling,
    Deathtouch,
    Defender,
    DoubleStrike,
    Enchant,
    Equip,
    FirstStrike,
    Flash,
    Flying,
    Haste,
    Hexproof,
    Indestructible,
    Intimidate,
    Landwalk,
    Lifelink,
    Menace,
    Protection,
    Prowess,
    Reach,
    Shroud,
    Trample,
    Vigilance,
    Ward,
    /// Any other keyword ability from CR 702, by its rules name.
    Other(String),
}

impl Keyword {
    fn from_rules_name(name: &str) -> Self {
        match name {
            "Cycling" => Keyword::Cycling,
            "Deathtouch" => Keyword::Deathtouch,
            "Defender" => Keyword::Defender,
            "Double Strike" => Keyword::DoubleStrike,
            "Enchant" => Keyword::Enchant,
            "Equip" => Keyword::Equip,
            "First Strike" => Keyword::FirstStrike,
            "Flash" => Keyword::Flash,
            "Flying" => Keyword::Flying,
            "Haste" => Keyword::Haste,
            "Hexproof" => Keyword::Hexproof,
            "Indestructible" => Keyword::Indestructible,
            "Intimidate" => Keyword::Intimidate,
            "Landwalk" => Keyword::Landwalk,
            "Lifelink" => Keyword::Lifelink,
            "Menace" => Keyword::Menace,
            "Protection" => Keyword::Protection,
            "Prowess" => Keyword::Prowess,
            "Reach" => Keyword::Reach,
            "Shroud" => Keyword::Shroud,
            "Trample" => Keyword::Trample,
            "Vigilance" => Keyword::Vigilance,
            "Ward" => Keyword::Ward,
            other => Keyword::Other(other.to_string()),
        }
    }

    /// The keyword's name as written in the Comprehensive Rules (CR 702).
    pub fn rules_name(&self) -> &str {
        match self {
            Keyword::Cycling => "Cycling",
            Keyword::Deathtouch => "Deathtouch",
            Keyword::Defender => "Defender",
            Keyword::DoubleStrike => "Double Strike",
            Keyword::Enchant => "Enchant",
            Keyword::Equip => "Equip",
            Keyword::FirstStrike => "First Strike",
            Keyword::Flash => "Flash",
            Keyword::Flying => "Flying",
            Keyword::Haste => "Haste",
            Keyword::Hexproof => "Hexproof",
            Keyword::Indestructible => "Indestructible",
            Keyword::Intimidate => "Intimidate",
            Keyword::Landwalk => "Landwalk",
            Keyword::Lifelink => "Lifelink",
            Keyword::Menace => "Menace",
            Keyword::Protection => "Protection",
            Keyword::Prowess => "Prowess",
            Keyword::Reach => "Reach",
            Keyword::Shroud => "Shroud",
            Keyword::Trample => "Trample",
            Keyword::Vigilance => "Vigilance",
            Keyword::Ward => "Ward",
            Keyword::Other(name) => name,
        }
    }
}

impl From<String> for Keyword {
    fn from(name: String) -> Self {
        Keyword::from_rules_name(&name)
    }
}

impl From<Keyword> for String {
    fn from(keyword: Keyword) -> Self {
        keyword.rules_name().to_string()
    }
}

/// One ability of a card (CR 113.3). Each paragraph of rules text is a
/// separate ability, except keywords strung together on one line (CR 113.2c).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Ability {
    /// A keyword ability (CR 702); `text` is the keyword as printed,
    /// including any parameter, e.g. `Ward {2}` or `Swampwalk`.
    Keyword { keyword: Keyword, text: String },
    /// `[Trigger condition], [effect]` (CR 113.3c, 603.1).
    Triggered { text: String },
    /// `[Cost]: [Effect]` (CR 113.3b, 602.1), including loyalty abilities (CR 606).
    Activated { cost: String, effect: String },
    /// A statement that is simply true (CR 113.3d).
    Static { text: String },
    /// Instructions followed as an instant or sorcery resolves (CR 113.3a).
    Spell { text: String },
}

impl Ability {
    /// Appends a continuation line (modal bullet, spree mode) to this ability.
    fn append(&mut self, line: &str) {
        let target = match self {
            Ability::Triggered { text } | Ability::Static { text } | Ability::Spell { text } => {
                text
            }
            Ability::Activated { effect, .. } => effect,
            Ability::Keyword { text, .. } => text,
        };
        target.push('\n');
        target.push_str(line);
    }
}

/// Splits oracle text into abilities. `is_spell` is true for instants and
/// sorceries, whose plain text lines are spell abilities rather than static ones.
pub fn parse_abilities(oracle_text: &str, is_spell: bool) -> Vec<Ability> {
    let mut abilities: Vec<Ability> = Vec::new();
    for raw in oracle_text.lines() {
        let line = strip_reminder_text(raw);
        if line.is_empty() {
            continue;
        }
        // Modes of a modal ability (CR 700.2) and spree (CR 702.172) belong to
        // the ability above them.
        if (line.starts_with('•') || line.starts_with("+ "))
            && let Some(last) = abilities.last_mut()
        {
            last.append(&line);
            continue;
        }
        let body = strip_ability_word(&line);

        if let Some(keywords) = parse_keyword_line(body) {
            abilities.extend(keywords);
        } else if is_triggered(body) {
            abilities.push(Ability::Triggered {
                text: body.to_string(),
            });
        } else if let Some((cost, effect)) = split_activated(body) {
            abilities.push(Ability::Activated {
                cost: cost.to_string(),
                effect: effect.to_string(),
            });
        } else if is_spell {
            abilities.push(Ability::Spell {
                text: body.to_string(),
            });
        } else {
            abilities.push(Ability::Static {
                text: body.to_string(),
            });
        }
    }
    abilities
}

/// Removes parenthesized reminder text (CR 207.2a).
fn strip_reminder_text(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth = 0usize;
    for c in line.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ability words (CR 207.2c) have no rules meaning; drop `Landfall — ` style prefixes.
fn strip_ability_word(line: &str) -> &str {
    match line.split_once(" — ") {
        Some((prefix, rest))
            if !rest.is_empty()
                && prefix.starts_with(|c: char| c.is_uppercase())
                && prefix.split_whitespace().count() <= 4
                && !prefix.contains(['{', ':', '.', ',', '"']) =>
        {
            rest
        }
        _ => line,
    }
}

fn is_triggered(line: &str) -> bool {
    ["When ", "Whenever ", "At "]
        .iter()
        .any(|w| line.starts_with(w))
}

/// Splits `[Cost]: [Effect]` on the first colon outside quoted text.
fn split_activated(line: &str) -> Option<(&str, &str)> {
    let mut in_quotes = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' | '“' | '”' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                let (cost, effect) = (line[..i].trim(), line[i + 1..].trim());
                return (!cost.is_empty() && !effect.is_empty()).then_some((cost, effect));
            }
            _ => {}
        }
    }
    None
}

/// Recognizes a line made only of keyword abilities, e.g. `Flying, vigilance`,
/// `Ward {2}`, `Basic landcycling {1}{G}` or `Ward—Pay 3 life.`
fn parse_keyword_line(line: &str) -> Option<Vec<Ability>> {
    // `[Keyword]—[cost]` form: the cost is a full sentence and may contain commas.
    if let Some((name, _)) = line.split_once('—')
        && !name.ends_with(' ')
    {
        let keyword = match_keyword(name)?;
        return Some(vec![Ability::Keyword {
            keyword,
            text: line.to_string(),
        }]);
    }
    // Keyword lines never end in a period; sentences do.
    if line.ends_with('.') {
        return None;
    }
    line.split(", ")
        .map(|part| {
            let keyword = match_keyword(part)?;
            Some(Ability::Keyword {
                keyword,
                text: part.to_string(),
            })
        })
        .collect()
}

/// Matches the start of `part` against a keyword ability name.
fn match_keyword(part: &str) -> Option<Keyword> {
    let lower = part.to_lowercase();
    let name = lower.split(['{', '—']).next().unwrap_or("").trim();

    // CR 702.29e: "[Type]cycling" is a cycling ability.
    if name.ends_with("cycling") {
        return Some(Keyword::Cycling);
    }
    // CR 702.14: "[Type]walk" is a landwalk ability.
    if name.ends_with("walk") && !name.contains(' ') || name.ends_with(" landwalk") {
        return Some(Keyword::Landwalk);
    }

    KEYWORD_ABILITIES
        .iter()
        .filter(|kw| {
            let kw = kw.to_lowercase();
            lower.starts_with(&kw)
                && matches!(
                    lower[kw.len()..].chars().next(),
                    None | Some(' ' | '{' | '—')
                )
        })
        .max_by_key(|kw| kw.len())
        .map(|kw| Keyword::from_rules_name(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw(keyword: Keyword, text: &str) -> Ability {
        Ability::Keyword {
            keyword,
            text: text.to_string(),
        }
    }

    fn permanent(text: &str) -> Vec<Ability> {
        parse_abilities(text, false)
    }

    // CR 702.1: a keyword ability is listed by name alone.
    #[test]
    fn single_keyword() {
        assert_eq!(permanent("Flying"), vec![kw(Keyword::Flying, "Flying")]);
        assert_eq!(
            permanent("Double strike"),
            vec![kw(Keyword::DoubleStrike, "Double strike")]
        );
    }

    // CR 113.2c: keyword abilities may be strung together on one line.
    #[test]
    fn comma_joined_keywords() {
        assert_eq!(
            permanent("Flying, first strike, lifelink"),
            vec![
                kw(Keyword::Flying, "Flying"),
                kw(Keyword::FirstStrike, "first strike"),
                kw(Keyword::Lifelink, "lifelink"),
            ]
        );
    }

    // Flash must not be confused with Flashback, nor First Strike with Double Strike.
    #[test]
    fn keyword_prefixes_are_whole_words() {
        assert_eq!(
            permanent("Flashback {2}{R}"),
            vec![kw(Keyword::Other("Flashback".into()), "Flashback {2}{R}")]
        );
        assert_eq!(permanent("Flash"), vec![kw(Keyword::Flash, "Flash")]);
    }

    // CR 702.21a, 702.6, 702.5, 702.16: keywords with a parameter.
    #[test]
    fn keywords_with_parameters() {
        assert_eq!(permanent("Ward {2}"), vec![kw(Keyword::Ward, "Ward {2}")]);
        assert_eq!(
            permanent("Equip {1}"),
            vec![kw(Keyword::Equip, "Equip {1}")]
        );
        assert_eq!(
            permanent("Enchant creature"),
            vec![kw(Keyword::Enchant, "Enchant creature")]
        );
        assert_eq!(
            permanent("Protection from red"),
            vec![kw(Keyword::Protection, "Protection from red")]
        );
        assert_eq!(
            permanent("Annihilator 2"),
            vec![kw(Keyword::Other("Annihilator".into()), "Annihilator 2")]
        );
    }

    // CR 702.21a: a keyword whose cost is a sentence, written with a long dash.
    #[test]
    fn keyword_with_dash_cost() {
        assert_eq!(
            permanent("Ward—Pay 3 life."),
            vec![kw(Keyword::Ward, "Ward—Pay 3 life.")]
        );
        assert_eq!(
            permanent("Flashback—{1}{R}, Discard a card."),
            vec![kw(
                Keyword::Other("Flashback".into()),
                "Flashback—{1}{R}, Discard a card."
            )]
        );
    }

    // CR 702.29e: typecycling is a cycling ability.
    #[test]
    fn typecycling() {
        assert_eq!(
            permanent("Cycling {2}"),
            vec![kw(Keyword::Cycling, "Cycling {2}")]
        );
        assert_eq!(
            permanent("Swampcycling {2}"),
            vec![kw(Keyword::Cycling, "Swampcycling {2}")]
        );
        assert_eq!(
            permanent("Basic landcycling {1}{G}"),
            vec![kw(Keyword::Cycling, "Basic landcycling {1}{G}")]
        );
    }

    // CR 702.14: landwalk is written as "[type]walk".
    #[test]
    fn landwalk() {
        assert_eq!(
            permanent("Swampwalk"),
            vec![kw(Keyword::Landwalk, "Swampwalk")]
        );
        assert_eq!(
            permanent("Nonbasic landwalk"),
            vec![kw(Keyword::Landwalk, "Nonbasic landwalk")]
        );
    }

    // CR 207.2a: reminder text is parenthesized and has no rules meaning.
    #[test]
    fn reminder_text_is_ignored() {
        assert_eq!(
            permanent(
                "Trample (This creature can deal excess combat damage to the player or planeswalker it's attacking.)"
            ),
            vec![kw(Keyword::Trample, "Trample")]
        );
        // A colon inside reminder text must not make the line an activated ability.
        assert_eq!(
            permanent(
                "Equip {2} ({2}: Attach to target creature you control. Equip only as a sorcery.)"
            ),
            vec![kw(Keyword::Equip, "Equip {2}")]
        );
        // Lines made only of reminder text produce no ability.
        assert!(permanent("(Melds with Some Card.)").is_empty());
    }

    // CR 113.3c, 603.1: triggered abilities begin with "when," "whenever," or "at."
    #[test]
    fn triggered_abilities() {
        for text in [
            "When this creature enters, draw a card.",
            "Whenever this creature attacks, it gets +1/+0 until end of turn.",
            "At the beginning of your upkeep, you gain 1 life.",
        ] {
            assert_eq!(
                permanent(text),
                vec![Ability::Triggered {
                    text: text.to_string()
                }]
            );
        }
    }

    // CR 207.2c: ability words are dropped, leaving the ability itself.
    #[test]
    fn ability_word_prefix() {
        assert_eq!(
            permanent(
                "Landfall — Whenever a land you control enters, put a +1/+1 counter on this creature."
            ),
            vec![Ability::Triggered {
                text: "Whenever a land you control enters, put a +1/+1 counter on this creature."
                    .into()
            }]
        );
    }

    // CR 113.3b, 602.1: activated abilities are written "[Cost]: [Effect.]"
    #[test]
    fn activated_abilities() {
        assert_eq!(
            permanent("{T}: Add {G}."),
            vec![Ability::Activated {
                cost: "{T}".into(),
                effect: "Add {G}.".into()
            }]
        );
        assert_eq!(
            permanent("{2}{B}, {T}, Sacrifice a creature: Draw two cards."),
            vec![Ability::Activated {
                cost: "{2}{B}, {T}, Sacrifice a creature".into(),
                effect: "Draw two cards.".into()
            }]
        );
    }

    // CR 606.2: loyalty abilities have a loyalty symbol in their cost.
    #[test]
    fn loyalty_abilities() {
        assert_eq!(
            permanent("+1: Draw a card.\n−3: Destroy target creature.\n[0]: Scry 1."),
            vec![
                Ability::Activated {
                    cost: "+1".into(),
                    effect: "Draw a card.".into()
                },
                Ability::Activated {
                    cost: "−3".into(),
                    effect: "Destroy target creature.".into()
                },
                Ability::Activated {
                    cost: "[0]".into(),
                    effect: "Scry 1.".into()
                },
            ]
        );
    }

    // A colon inside a granted, quoted ability belongs to that ability.
    #[test]
    fn quoted_colon_is_not_activated() {
        let text = "Creatures you control have \"{T}: Add one mana of any color.\"";
        assert_eq!(permanent(text), vec![Ability::Static { text: text.into() }]);
    }

    // CR 113.3d: static abilities are statements.
    #[test]
    fn static_abilities() {
        let text = "Other creatures you control get +1/+1.";
        assert_eq!(permanent(text), vec![Ability::Static { text: text.into() }]);
    }

    // A sentence that starts with a keyword name is not a keyword line.
    #[test]
    fn sentences_starting_with_keyword_names() {
        let text = "Flying creatures you control get +1/+1.";
        assert_eq!(permanent(text), vec![Ability::Static { text: text.into() }]);
    }

    // CR 113.3a: text on an instant or sorcery is a spell ability.
    #[test]
    fn spell_abilities() {
        assert_eq!(
            parse_abilities("Lightning Bolt deals 3 damage to any target.", true),
            vec![Ability::Spell {
                text: "Lightning Bolt deals 3 damage to any target.".into()
            }]
        );
        // Keyword abilities on instants are still keywords.
        assert_eq!(
            parse_abilities("Flash\nCounter target spell.", true),
            vec![
                kw(Keyword::Flash, "Flash"),
                Ability::Spell {
                    text: "Counter target spell.".into()
                },
            ]
        );
    }

    // CR 113.2c: each paragraph is a separate ability.
    #[test]
    fn one_ability_per_paragraph() {
        let abilities = permanent(
            "Flying\nWhen this creature enters, scry 2.\n{1}{U}: This creature gets +1/-1 until end of turn.\nThis creature can't block.",
        );
        assert!(matches!(
            abilities[0],
            Ability::Keyword {
                keyword: Keyword::Flying,
                ..
            }
        ));
        assert!(matches!(abilities[1], Ability::Triggered { .. }));
        assert!(matches!(abilities[2], Ability::Activated { .. }));
        assert!(matches!(abilities[3], Ability::Static { .. }));
    }

    // CR 700.2: modes of a modal spell belong to the same ability.
    #[test]
    fn modal_bullets_join_their_ability() {
        assert_eq!(
            parse_abilities("Choose one —\n• Draw a card.\n• Gain 3 life.", true),
            vec![Ability::Spell {
                text: "Choose one —\n• Draw a card.\n• Gain 3 life.".into()
            }]
        );
    }
}
