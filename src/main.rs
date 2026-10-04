use std::path::Path;
use std::process::ExitCode;

use download_database::scryfall::{self, SetInfo};
use download_database::store;

const USAGE: &str = "\
usage:
  download-database sets [--all] [--json]   list sets (playable paper sets unless --all)
  download-database download [CODE]...      download sets to data/<code>/ (default: fra)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("sets") => sets(&args[1..]),
        Some("download") => download(&args[1..]),
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
