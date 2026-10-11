use std::path::Path;
use std::process::ExitCode;

use download_database::Card;
use download_database::deck::{DeckStore, TomlDeckStore};
use download_database::game::{Action, DecisionKind, Event, Game, GameResult, ScriptPool, Target};
use download_database::pool::CardPool;
use download_database::scryfall::{self, SetInfo};
use download_database::store;

const USAGE: &str = "\
usage:
  download-database sets [--all] [--json]   list sets (playable paper sets unless --all)
  download-database download [CODE]...      download sets to data/<code>/ (default: fra)
  download-database playable [CODE]...      list which downloaded cards the game engine can play
  download-database play DECK DECK [--seed N]
                                            play two decks (saved names or .toml paths)
                                            against each other with players who pick
                                            random legal actions";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("sets") => sets(&args[1..]),
        Some("download") => download(&args[1..]),
        Some("playable") => playable(&args[1..]),
        Some("play") => play(&args[1..]),
        Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn sets(flags: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut all = false;
    let mut json = false;
    for flag in flags {
        match flag.as_str() {
            "--all" => all = true,
            "--json" => json = true,
            other => return Err(format!("unknown flag {other:?}\n{USAGE}").into()),
        }
    }

    let sets: Vec<SetInfo> = scryfall::list_sets()?
        .into_iter()
        .filter(|s| all || s.is_playable())
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&sets)?);
        return Ok(());
    }
    println!(
        "{:<6}  {:<10}  {:<16}  {:>5}  NAME",
        "CODE", "RELEASED", "TYPE", "CARDS"
    );
    for s in &sets {
        println!(
            "{:<6}  {:<10}  {:<16}  {:>5}  {}",
            s.code,
            s.released_at.as_deref().unwrap_or("-"),
            s.set_type,
            s.card_count,
            s.name
        );
    }
    Ok(())
}

fn download(codes: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let default = [scryfall::REALITY_FRACTURE.to_string()];
    let codes = if codes.is_empty() {
        &default[..]
    } else {
        codes
    };
    for code in codes {
        let report = scryfall::fetch_set(code)?;
        let code = code.to_ascii_lowercase();
        println!(
            "{code}: {} cards parsed, {} skipped (of {} on Scryfall)",
            report.cards.len(),
            report.skipped.len(),
            report.total_cards
        );
        for skipped in &report.skipped {
            println!("  skipped {}: {}", skipped.name, skipped.reason);
        }

        let dir = store::set_dir(Path::new("data"), &code);
        store::write_set(&dir, &report.cards)?;
        println!("wrote {} files to {}/", report.cards.len(), dir.display());
    }
    Ok(())
}

fn playable(codes: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let pool = CardPool::load(Path::new("data"))?;
    let scripts = ScriptPool::load(Path::new("scripts"))?;
    let codes: Vec<String> = codes.iter().map(|c| c.to_ascii_lowercase()).collect();
    let mut total = 0;
    let mut ok = 0;
    for card in pool.cards() {
        if !codes.is_empty() && !codes.contains(&card.set) {
            continue;
        }
        total += 1;
        match scripts.unplayable(card) {
            None => {
                ok += 1;
                println!("ok  {} ({} {})", card.name, card.set, card.collector_number);
            }
            Some(why) => println!(
                "--  {} ({} {}): {why}",
                card.name, card.set, card.collector_number
            ),
        }
    }
    println!("{ok} of {total} cards are playable");
    Ok(())
}

fn play(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut names = Vec::new();
    let mut seed = 0u64;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--seed" => {
                seed = rest
                    .next()
                    .ok_or_else(|| format!("--seed needs a number\n{USAGE}"))?
                    .parse()?;
            }
            name => names.push(name.to_string()),
        }
    }
    let [first, second] = names.as_slice() else {
        return Err(format!("play needs two deck names\n{USAGE}").into());
    };

    let pool = CardPool::load(Path::new("data"))?;
    let scripts = ScriptPool::load(Path::new("scripts"))?;
    let store = TomlDeckStore::new("decks");
    let mut decks: [Vec<Card>; 2] = [Vec::new(), Vec::new()];
    for (deck, name) in decks.iter_mut().zip([first, second]) {
        // A deck saved in decks/, or a path to a deck file.
        let deck_file = Path::new(name.as_str());
        let saved = if deck_file.is_file() {
            toml::from_str::<download_database::Deck>(&std::fs::read_to_string(deck_file)?)?
        } else {
            store.load(name)?
        };
        for entry in saved.main {
            let card = pool
                .get(&entry.card)
                .ok_or_else(|| format!("{name}: {} isn't downloaded", entry.card.name))?;
            deck.extend(std::iter::repeat_n(card.clone(), entry.count as usize));
        }
    }

    let mut game = Game::new(decks, &scripts, seed)?;
    let mut random = seed ^ 0x5eed;
    let mut printed = 0;
    while let Some(decision) = game.pending().cloned() {
        let action = match decision.kind {
            DecisionKind::Mulligan { .. } => Action::Keep,
            _ => {
                let actions = game.legal_actions();
                random = random
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let pick = (random >> 33) as usize;
                // Pass priority more often than not, so the game moves along.
                if decision.kind == DecisionKind::Priority && pick.is_multiple_of(3) {
                    Action::Pass
                } else {
                    actions[pick % actions.len()].clone()
                }
            }
        };
        game.apply(decision.player, action)?;
        for event in &game.events()[printed..] {
            if let Some(line) = describe(&game, event) {
                println!("{line}");
            }
        }
        printed = game.events().len();
    }
    match game.result() {
        Some(GameResult::Winner(p)) => println!("player {p} ({}) wins", [first, second][p]),
        Some(GameResult::Draw) => println!("the game is a draw"),
        None => {}
    }
    Ok(())
}

/// A line for the game log, for the events worth showing.
fn describe(game: &Game, event: &Event) -> Option<String> {
    let name = |id| game.object_view(id).name;
    let target = |t: &Target| match t {
        Target::Player(p) => format!("player {p}"),
        Target::Object(id) => name(*id),
    };
    Some(match event {
        Event::TurnBegan { turn, player } => format!("--- turn {turn}: player {player}"),
        Event::LandPlayed { player, card } => format!("player {player} plays {}", name(*card)),
        Event::SpellCast { player, spell } => format!("player {player} casts {}", name(*spell)),
        Event::AttackerDeclared { attacker } => format!("{} attacks", name(*attacker)),
        Event::BlockerDeclared { blocker, attacker } => {
            format!("{} blocks {}", name(*blocker), name(*attacker))
        }
        Event::DamageDealt {
            source,
            target: t,
            amount,
            ..
        } => format!("{} deals {amount} damage to {}", name(*source), target(t)),
        Event::PlayerLost { player, reason } => format!("player {player} loses ({reason:?})"),
        _ => return None,
    })
}
