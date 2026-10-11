//! Engine tests, each checking a rule of the Comprehensive Rules.

use super::action::{Action, DecisionKind, ManaActivation};
use super::event::{Event, LossReason};
use super::mana::Mana;
use super::script::{CardScript, ScriptPool};
use super::state::{GameResult, ObjectId, Target, Zone};
use super::testing::*;
use super::turn::Step;
use super::{Game, GameError};
use crate::card::Card;

// ---------------------------------------------------------------------------
// Test cards

fn shock() -> CardScript {
    script(
        r#"
        name = "Shock"
        [spell]
        effect = "deal_damage"
        amount = 2
        target = { kind = "any" }
        "#,
    )
}

fn unsummon() -> CardScript {
    script(
        r#"
        name = "Unsummon"
        [spell]
        effect = "return_to_hand"
        target = { kind = "creature" }
        "#,
    )
}

fn negate() -> CardScript {
    script(
        r#"
        name = "Negate"
        [spell]
        effect = "counter_spell"
        target = { kind = "spell" }
        "#,
    )
}

fn bear() -> Card {
    creature("Bear", "{1}{G}", 2, 2, "")
}

fn names(game: &Game, ids: &[ObjectId]) -> Vec<String> {
    ids.iter()
        .map(|id| game.card(*id).unwrap().name.clone())
        .collect()
}

fn on_battlefield(game: &Game, name: &str) -> Option<ObjectId> {
    game.battlefield()
        .iter()
        .copied()
        .find(|id| game.card(*id).unwrap().name == name)
}

/// Casts `card` with `targets`, using the engine's suggested mana payment.
fn cast(game: &mut Game, card: ObjectId, targets: Vec<Target>) {
    let action = game
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::CastSpell { card: c, targets: t, .. } if *c == card && *t == targets))
        .unwrap_or_else(|| panic!("can't cast {card} at {targets:?}"));
    act(game, action);
}

fn activate(game: &mut Game, source: ObjectId, targets: Vec<Target>) {
    let action = game
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::ActivateAbility { source: s, targets: t, .. } if *s == source && *t == targets))
        .unwrap_or_else(|| panic!("can't activate {source}"));
    act(game, action);
}

fn can_cast(game: &Game, card: ObjectId) -> bool {
    game.legal_actions()
        .iter()
        .any(|a| matches!(a, Action::CastSpell { card: c, .. } if *c == card))
}

fn stepped_through(game: &Game, step: Step) -> bool {
    game.events().contains(&Event::StepBegan { step })
}

fn deck(card: Card, n: usize) -> Vec<Card> {
    vec![card; n]
}

// ---------------------------------------------------------------------------
// Starting the game (CR 103)

// CR 103.1, 103.4, 103.5: a player chooses who starts, everyone starts at 20
// life and draws seven, and the starting player declares mulligans first.
#[test]
fn starting_the_game() {
    let mut game = Game::new(
        [deck(basic("Forest"), 40), deck(basic("Island"), 40)],
        &ScriptPool::default(),
        1,
    )
    .unwrap();
    assert_eq!(decision(&game), &DecisionKind::ChooseStartingPlayer);
    act(&mut game, Action::ChooseStartingPlayer { player: 1 });
    assert_eq!(game.hand(0).len(), 7);
    assert_eq!(game.hand(1).len(), 7);
    assert_eq!(game.life(0), 20);
    assert_eq!(game.life(1), 20);
    assert_eq!(game.pending().unwrap().player, 1);
    assert!(matches!(decision(&game), DecisionKind::Mulligan { .. }));
}

// CR 103.5: a mulligan draws a new seven and puts one card on the bottom per
// mulligan taken.
#[test]
fn london_mulligan() {
    let mut game = Game::new(
        [deck(basic("Forest"), 40), deck(basic("Island"), 40)],
        &ScriptPool::default(),
        1,
    )
    .unwrap();
    act(&mut game, Action::ChooseStartingPlayer { player: 0 });
    act(&mut game, Action::Mulligan);
    act(&mut game, Action::Keep);
    assert_eq!(game.hand(0).len(), 7);
    assert_eq!(decision(&game), &DecisionKind::PutOnBottom { count: 1 });
    let bottom = game.hand(0)[0];
    act(
        &mut game,
        Action::PutOnBottom {
            cards: vec![bottom],
        },
    );
    assert_eq!(game.hand(0).len(), 6);
    assert_eq!(names(&game, &game.library(0)[..1]), vec!["Forest"]);
    // The player who mulliganed declares again; the other already kept.
    assert_eq!(game.pending().unwrap().player, 0);
    act(&mut game, Action::Keep);
    assert_eq!(game.turn(), 1);
    assert_eq!(game.active_player(), 0);
}

// CR 103.8a: the starting player skips the draw step of their first turn.
#[test]
fn starting_player_skips_first_draw() {
    let mut game = Game::new(
        [deck(basic("Forest"), 40), deck(basic("Island"), 40)],
        &ScriptPool::default(),
        1,
    )
    .unwrap();
    act(&mut game, Action::ChooseStartingPlayer { player: 0 });
    act(&mut game, Action::Keep);
    act(&mut game, Action::Keep);
    pass_until(&mut game, Step::PrecombatMain);
    assert_eq!(game.hand(0).len(), 7);
    assert!(!stepped_through(&game, Step::Draw));
    // The other player draws on their first turn.
    finish_turn(&mut game);
    pass_until(&mut game, Step::PrecombatMain);
    assert_eq!(game.hand(1).len(), 8);
}

#[test]
fn unplayable_cards_are_rejected() {
    let bolt = instant("Mystery Bolt", "{R}");
    let err = Game::new([vec![bolt], vec![]], &ScriptPool::default(), 1).unwrap_err();
    assert!(matches!(err, GameError::Unplayable(ref cards) if cards[0].0 == "Mystery Bolt"));
}

// The same seed shuffles the same way.
#[test]
fn games_are_reproducible() {
    let decks = || {
        let cards: Vec<Card> = (0..40)
            .map(|i| creature(&format!("C{i}"), "{G}", 1, 1, ""))
            .collect();
        [cards.clone(), cards]
    };
    let a = Game::new(decks(), &ScriptPool::default(), 9).unwrap();
    let b = Game::new(decks(), &ScriptPool::default(), 9).unwrap();
    assert_eq!(names(&a, a.library(0)), names(&b, b.library(0)));
}

// ---------------------------------------------------------------------------
// Turn structure (CR 500–514)

// CR 502.3: the active player untaps their permanents; others stay tapped.
#[test]
fn untap_step_untaps_active_players_permanents() {
    let mut b = GameBuilder::new(vec![]);
    b.libraries(5);
    let mine = b.battlefield(0, basic("Forest"));
    let theirs = b.battlefield(1, basic("Forest"));
    b.game.objects.get_mut(&mine).unwrap().tapped = true;
    b.game.objects.get_mut(&theirs).unwrap().tapped = true;
    let game = b.start(Step::Untap);
    // CR 502.4: no priority in the untap step; upkeep is next.
    assert_eq!(game.step(), Step::Upkeep);
    assert!(!game.is_tapped(mine));
    assert!(game.is_tapped(theirs));
}

// CR 504.1: the active player draws a card in their draw step.
#[test]
fn draw_step_draws() {
    let mut b = GameBuilder::new(vec![]);
    b.libraries(5);
    let game = b.start(Step::Draw);
    assert_eq!(game.hand(0).len(), 1);
    assert_eq!(game.hand(1).len(), 0);
}

// CR 704.5b: a player who draws from an empty library loses.
#[test]
fn drawing_from_empty_library_loses() {
    let game = GameBuilder::new(vec![]).start(Step::Draw);
    assert_eq!(game.result(), Some(GameResult::Winner(1)));
    assert!(game.events().contains(&Event::PlayerLost {
        player: 0,
        reason: LossReason::DrewFromEmptyLibrary
    }));
}

// CR 514.1: the active player discards down to seven in the cleanup step.
#[test]
fn cleanup_discards_to_hand_size() {
    let mut b = GameBuilder::new(vec![]);
    b.libraries(5);
    for _ in 0..9 {
        b.hand(0, basic("Forest"));
    }
    let mut game = b.start(Step::Cleanup);
    assert_eq!(decision(&game), &DecisionKind::Discard { count: 2 });
    let cards = game.hand(0)[..2].to_vec();
    act(&mut game, Action::Discard { cards });
    assert_eq!(game.hand(0).len(), 7);
    assert_eq!(game.graveyard(0).len(), 2);
    // The turn ends and the other player's begins.
    assert_eq!(game.active_player(), 1);
}

// CR 117.4, 500.2: when both players pass with an empty stack, the step ends.
#[test]
fn passing_with_empty_stack_ends_the_step() {
    let mut b = GameBuilder::new(vec![]);
    b.libraries(5);
    let mut game = b.start(Step::Upkeep);
    assert_eq!(game.pending().unwrap().player, 0);
    pass(&mut game);
    assert_eq!(game.step(), Step::Upkeep);
    assert_eq!(game.pending().unwrap().player, 1);
    pass(&mut game);
    assert_eq!(game.step(), Step::Draw);
}

// CR 500.5: unspent mana empties from mana pools as a step ends.
#[test]
fn mana_empties_between_steps() {
    let mut b = GameBuilder::new(vec![]);
    b.libraries(5);
    let forest = b.battlefield(0, basic("Forest"));
    let mut game = b.start(Step::PrecombatMain);
    act(
        &mut game,
        Action::ActivateManaAbility(ManaActivation {
            source: forest,
            mana: vec![Mana::Green],
        }),
    );
    assert_eq!(game.mana_pool(0).get(Mana::Green), 1);
    pass(&mut game);
    pass(&mut game);
    assert_eq!(game.step(), Step::BeginningOfCombat);
    assert!(game.mana_pool(0).is_empty());
}

// ---------------------------------------------------------------------------
// Lands and mana (CR 305, 605)

// CR 305.2: a player can normally play one land per turn.
#[test]
fn one_land_per_turn() {
    let mut b = GameBuilder::new(vec![]);
    let first = b.hand(0, basic("Forest"));
    let second = b.hand(0, basic("Forest"));
    let mut game = b.start(Step::PrecombatMain);
    act(&mut game, Action::PlayLand { card: first });
    assert_eq!(game.battlefield().len(), 1);
    assert!(game.apply(0, Action::PlayLand { card: second }).is_err());
    assert!(
        !game
            .legal_actions()
            .contains(&Action::PlayLand { card: second })
    );
}

// CR 305.3: a player can't play a land if it isn't their turn.
#[test]
fn no_lands_on_opponents_turn() {
    let mut b = GameBuilder::new(vec![]);
    let land = b.hand(1, basic("Forest"));
    let mut game = b.start(Step::PrecombatMain);
    pass(&mut game);
    assert_eq!(game.pending().unwrap().player, 1);
    assert!(game.apply(1, Action::PlayLand { card: land }).is_err());
}

// CR 505.6b: lands are played only while the stack is empty.
#[test]
fn no_lands_while_the_stack_is_not_empty() {
    let mut b = GameBuilder::new(vec![shock()]);
    b.battlefield(0, basic("Mountain"));
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let land = b.hand(0, basic("Forest"));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, bolt, vec![Target::Player(1)]);
    // CR 117.3c: the caster gets priority again.
    assert_eq!(game.pending().unwrap().player, 0);
    assert!(game.apply(0, Action::PlayLand { card: land }).is_err());
}

// CR 305.6, 605.3b: a basic land taps for its color, without using the stack.
#[test]
fn basic_land_mana_ability() {
    let mut b = GameBuilder::new(vec![]);
    let island = b.battlefield(0, basic("Island"));
    let mut game = b.start(Step::PrecombatMain);
    let wrong = ManaActivation {
        source: island,
        mana: vec![Mana::Red],
    };
    assert!(game.apply(0, Action::ActivateManaAbility(wrong)).is_err());
    act(
        &mut game,
        Action::ActivateManaAbility(ManaActivation {
            source: island,
            mana: vec![Mana::Blue],
        }),
    );
    assert!(game.is_tapped(island));
    assert_eq!(game.mana_pool(0).get(Mana::Blue), 1);
    assert!(game.stack().is_empty());
}

// CR 605.3b, 302.6: a scripted mana ability on a creature doesn't use the
// stack, and can't be activated while the creature is summoning sick.
#[test]
fn creature_mana_ability() {
    let elf = script(
        r#"
        name = "Elf"
        [[mana]]
        produces = ["{G}"]
        "#,
    );
    let mut b = GameBuilder::new(vec![elf]);
    let ready = b.battlefield(0, creature("Elf", "{G}", 1, 1, "{T}: Add {G}."));
    let sick = b.battlefield(0, creature("Elf", "{G}", 1, 1, "{T}: Add {G}."));
    b.game.objects.get_mut(&sick).unwrap().summoning_sick = true;
    let mut game = b.start(Step::PrecombatMain);
    let tap = |source| {
        Action::ActivateManaAbility(ManaActivation {
            source,
            mana: vec![Mana::Green],
        })
    };
    assert!(game.apply(0, tap(sick)).is_err());
    act(&mut game, tap(ready));
    assert_eq!(game.mana_pool(0).get(Mana::Green), 1);
    assert!(game.stack().is_empty());
}

// ---------------------------------------------------------------------------
// Casting spells and resolving them (CR 117, 601, 608)

// CR 117.1a: noninstant spells only in your main phase with an empty stack;
// instants any time you have priority.
#[test]
fn spell_timing() {
    let mut b = GameBuilder::new(vec![shock()]);
    b.libraries(5);
    b.battlefield(0, basic("Mountain"));
    b.battlefield(0, basic("Forest"));
    let creature = b.hand(0, bear());
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let game = b.start(Step::Upkeep);
    assert!(!can_cast(&game, creature));
    assert!(can_cast(&game, bolt));
}

// CR 702.8a: flash lets a permanent spell be cast any time its controller
// could cast an instant.
#[test]
fn flash() {
    let mut b = GameBuilder::new(vec![]);
    b.libraries(5);
    b.battlefield(0, basic("Forest"));
    let flier = b.hand(0, creature("Flash Cat", "{G}", 1, 1, "Flash"));
    let game = b.start(Step::Upkeep);
    assert!(can_cast(&game, flier));
}

// CR 601.2h: a spell can't be cast without paying its cost.
#[test]
fn spells_need_their_mana() {
    let mut b = GameBuilder::new(vec![]);
    let forest = b.battlefield(0, basic("Forest"));
    let creature = b.hand(0, bear());
    let mut game = b.start(Step::PrecombatMain);
    assert!(!can_cast(&game, creature));
    let attempt = Action::CastSpell {
        card: creature,
        mode: None,
        targets: vec![],
        mana: vec![ManaActivation {
            source: forest,
            mana: vec![Mana::Green],
        }],
    };
    assert!(game.apply(0, attempt).is_err());
    assert_eq!(game.hand(0), &[creature]);
}

// CR 601.2, 608.3a: a creature spell goes on the stack, and becomes a
// permanent under its controller's control as it resolves.
#[test]
fn creature_spell_resolves_onto_the_battlefield() {
    let mut b = GameBuilder::new(vec![]);
    let forests = [
        b.battlefield(0, basic("Forest")),
        b.battlefield(0, basic("Forest")),
    ];
    let creature = b.hand(0, bear());
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, creature, vec![]);
    assert!(forests.iter().all(|f| game.is_tapped(*f)));
    assert_eq!(names(&game, &game.stack()), vec!["Bear"]);
    pass(&mut game);
    pass(&mut game);
    let bear = on_battlefield(&game, "Bear").unwrap();
    assert_eq!(game.controller(bear), Some(0));
    assert!(game.stack().is_empty());
    // CR 117.3b: the active player gets priority after a spell resolves.
    assert_eq!(game.pending().unwrap().player, 0);
}

// CR 405.5, 117.7: the last spell cast resolves first.
#[test]
fn stack_is_last_in_first_out() {
    let mut b = GameBuilder::new(vec![shock()]);
    b.battlefield(0, basic("Mountain"));
    b.battlefield(1, basic("Mountain"));
    let mine = b.hand(0, instant("Shock", "{R}"));
    let theirs = b.hand(1, instant("Shock", "{R}"));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, mine, vec![Target::Player(1)]);
    pass(&mut game);
    cast(&mut game, theirs, vec![Target::Player(0)]);
    resolve_stack(&mut game);
    let hits: Vec<Target> = game
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::DamageDealt { target, .. } => Some(*target),
            _ => None,
        })
        .collect();
    assert_eq!(hits, vec![Target::Player(0), Target::Player(1)]);
    assert_eq!((game.life(0), game.life(1)), (18, 18));
}

// CR 608.2b: a spell whose only target became illegal doesn't resolve.
#[test]
fn spell_fizzles_when_its_target_is_gone() {
    let mut b = GameBuilder::new(vec![shock(), unsummon()]);
    b.battlefield(0, basic("Mountain"));
    b.battlefield(1, basic("Island"));
    let target = b.battlefield(1, bear());
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let bounce = b.hand(1, instant("Unsummon", "{U}"));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, bolt, vec![Target::Object(target)]);
    pass(&mut game);
    cast(&mut game, bounce, vec![Target::Object(target)]);
    resolve_stack(&mut game);
    assert_eq!(names(&game, game.hand(1)), vec!["Bear"]);
    assert!(
        game.events()
            .iter()
            .any(|e| matches!(e, Event::Fizzled { .. }))
    );
    assert_eq!(names(&game, game.graveyard(0)), vec!["Shock"]);
    assert!(
        !game
            .events()
            .iter()
            .any(|e| matches!(e, Event::DamageDealt { .. }))
    );
}

// CR 701.6a: a countered spell goes to its owner's graveyard without
// resolving.
#[test]
fn counterspell() {
    let mut b = GameBuilder::new(vec![shock(), negate()]);
    b.battlefield(0, basic("Mountain"));
    b.battlefield(1, basic("Island"));
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let counter = b.hand(1, instant("Negate", "{U}"));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, bolt, vec![Target::Player(1)]);
    let spell = game.stack()[0];
    pass(&mut game);
    cast(&mut game, counter, vec![Target::Object(spell)]);
    resolve_stack(&mut game);
    assert_eq!(game.life(1), 20);
    assert_eq!(names(&game, game.graveyard(0)), vec!["Shock"]);
    assert_eq!(names(&game, game.graveyard(1)), vec!["Negate"]);
}

// CR 702.11b: a permanent with hexproof can't be the target of spells or
// abilities its opponents control, but its controller can target it.
#[test]
fn hexproof() {
    let mut b = GameBuilder::new(vec![shock(), unsummon()]);
    b.battlefield(0, basic("Mountain"));
    b.battlefield(1, basic("Island"));
    let warded = b.battlefield(1, creature("Hexer", "{G}", 2, 2, "Hexproof"));
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let bounce = b.hand(1, instant("Unsummon", "{U}"));
    let mut game = b.start(Step::PrecombatMain);
    let shock_at = Action::CastSpell {
        card: bolt,
        mode: None,
        targets: vec![Target::Object(warded)],
        mana: vec![],
    };
    assert!(!game.legal_actions().iter().any(|a| matches!(a, Action::CastSpell { targets, .. } if targets == &vec![Target::Object(warded)])));
    assert!(game.clone().apply(0, shock_at).is_err());
    pass(&mut game);
    cast(&mut game, bounce, vec![Target::Object(warded)]);
}

// CR 702.18a: a permanent with shroud can't be the target of spells or
// abilities, even its controller's.
#[test]
fn shroud() {
    let mut b = GameBuilder::new(vec![unsummon()]);
    b.battlefield(0, basic("Island"));
    let shrouded = b.battlefield(0, creature("Shrouded", "{G}", 2, 2, "Shroud"));
    let bounce = b.hand(0, instant("Unsummon", "{U}"));
    let game = b.start(Step::PrecombatMain);
    assert!(!game.legal_actions().iter().any(|a| matches!(a, Action::CastSpell { card, targets, .. } if *card == bounce && targets == &vec![Target::Object(shrouded)])));
}

// CR 700.2: a modal spell's mode is chosen as it's cast.
#[test]
fn modal_spell() {
    let charm = script(
        r#"
        name = "Charm"
        [spell]
        effect = "choose_one"
        [[spell.modes]]
        effect = "gain_life"
        amount = 4
        [[spell.modes]]
        effect = "deal_damage"
        amount = 2
        target = { kind = "player" }
        "#,
    );
    let mut b = GameBuilder::new(vec![charm]);
    b.battlefield(0, basic("Mountain"));
    let card = b.hand(0, instant("Charm", "{R}"));
    let mut game = b.start(Step::PrecombatMain);
    let modes: Vec<Option<usize>> = game
        .legal_actions()
        .iter()
        .filter_map(|a| match a {
            Action::CastSpell { mode, .. } => Some(*mode),
            _ => None,
        })
        .collect();
    // One "gain life" choice, two "damage to target player" choices.
    assert_eq!(modes, vec![Some(0), Some(1), Some(1)]);
    let action = game
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::CastSpell { mode: Some(0), .. }))
        .unwrap();
    act(&mut game, action);
    resolve_stack(&mut game);
    assert_eq!(game.life(0), 24);
    let _ = card;
}

// CR 701.9b: the discarding player chooses which cards to discard.
#[test]
fn discard_is_the_players_choice() {
    let mind_rot = script(
        r#"
        name = "Mind Rot"
        [spell]
        effect = "discard"
        amount = 2
        player = { kind = "player" }
        "#,
    );
    let mut b = GameBuilder::new(vec![mind_rot]);
    b.battlefield(0, basic("Swamp"));
    let card = b.hand(0, sorcery("Mind Rot", "{B}"));
    let kept = b.hand(1, basic("Forest"));
    let tossed = [b.hand(1, basic("Island")), b.hand(1, basic("Swamp"))];
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, card, vec![Target::Player(1)]);
    pass(&mut game);
    pass(&mut game);
    assert_eq!(game.pending().unwrap().player, 1);
    assert_eq!(decision(&game), &DecisionKind::Discard { count: 2 });
    assert!(
        game.apply(1, Action::Discard { cards: vec![kept] })
            .is_err()
    );
    act(
        &mut game,
        Action::Discard {
            cards: tossed.to_vec(),
        },
    );
    assert_eq!(game.hand(1), &[kept]);
    assert_eq!(names(&game, game.graveyard(0)), vec!["Mind Rot"]);
}

// ---------------------------------------------------------------------------
// State-based actions (CR 704)

// CR 704.5a: a player with 0 or less life loses the game.
#[test]
fn zero_life_loses() {
    let mut b = GameBuilder::new(vec![shock()]);
    b.battlefield(0, basic("Mountain"));
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let mut game = b.start(Step::PrecombatMain);
    game.players[1].life = 2;
    cast(&mut game, bolt, vec![Target::Player(1)]);
    resolve_stack(&mut game);
    assert_eq!(game.result(), Some(GameResult::Winner(0)));
    assert!(game.pending().is_none());
}

// CR 104.4a: if all players lose simultaneously, the game is a draw.
#[test]
fn everyone_losing_is_a_draw() {
    let quake = script(
        r#"
        name = "Quake"
        [spell]
        effect = "deal_damage"
        amount = 1
        target = "each_player"
        "#,
    );
    let mut b = GameBuilder::new(vec![quake]);
    b.battlefield(0, basic("Mountain"));
    let card = b.hand(0, sorcery("Quake", "{R}"));
    let mut game = b.start(Step::PrecombatMain);
    game.players[0].life = 1;
    game.players[1].life = 1;
    cast(&mut game, card, vec![]);
    resolve_stack(&mut game);
    assert_eq!(game.result(), Some(GameResult::Draw));
}

// CR 704.5f: a creature with toughness 0 or less is put into its owner's
// graveyard, even if it has indestructible.
#[test]
fn zero_toughness_dies_even_if_indestructible() {
    let mut b = GameBuilder::new(vec![]);
    b.battlefield(0, creature("Wisp", "{W}", 1, 0, "Indestructible"));
    let game = b.start(Step::PrecombatMain);
    assert!(game.battlefield().is_empty());
    assert_eq!(names(&game, game.graveyard(0)), vec!["Wisp"]);
}

// CR 704.5g, 702.12b: lethal damage destroys a creature, unless it has
// indestructible.
#[test]
fn lethal_damage_destroys() {
    let mut b = GameBuilder::new(vec![shock()]);
    b.battlefield(0, basic("Mountain"));
    b.battlefield(0, basic("Mountain"));
    let target = b.battlefield(1, bear());
    let sturdy = b.battlefield(1, creature("Sturdy", "{G}", 2, 2, "Indestructible"));
    let bolts = [
        b.hand(0, instant("Shock", "{R}")),
        b.hand(0, instant("Shock", "{R}")),
    ];
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, bolts[0], vec![Target::Object(target)]);
    resolve_stack(&mut game);
    cast(&mut game, bolts[1], vec![Target::Object(sturdy)]);
    resolve_stack(&mut game);
    assert_eq!(names(&game, game.graveyard(1)), vec!["Bear"]);
    assert_eq!(game.damage(sturdy), 2);
    assert!(game.battlefield().contains(&sturdy));
}

// CR 704.5j: with two legendary permanents of the same name, the controller
// chooses one to keep.
#[test]
fn legend_rule() {
    let mut b = GameBuilder::new(vec![]);
    b.battlefield(0, basic("Forest"));
    let old = b.battlefield(0, legendary_creature("Captain", "{G}", 2, 2));
    let new = b.hand(0, legendary_creature("Captain", "{G}", 2, 2));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, new, vec![]);
    pass(&mut game);
    pass(&mut game);
    let DecisionKind::LegendRule { choices } = decision(&game).clone() else {
        panic!("expected the legend rule");
    };
    assert!(choices.contains(&old));
    let keep = *choices.iter().find(|c| **c != old).unwrap();
    act(&mut game, Action::KeepLegend { card: keep });
    assert!(game.battlefield().contains(&keep));
    assert!(!game.battlefield().contains(&old));
    assert_eq!(names(&game, game.graveyard(0)), vec!["Captain"]);
}

// CR 111.7, 704.5d: a token that leaves the battlefield ceases to exist.
#[test]
fn tokens_cease_to_exist_off_the_battlefield() {
    let goblins = script(
        r#"
        name = "Make Goblin"
        [spell]
        effect = "create_token"
        token = { name = "Goblin", type_line = "Creature — Goblin", colors = ["red"], power = 1, toughness = 1 }
        "#,
    );
    let mut b = GameBuilder::new(vec![goblins, shock()]);
    b.battlefield(0, basic("Mountain"));
    b.battlefield(0, basic("Mountain"));
    let make = b.hand(0, sorcery("Make Goblin", "{R}"));
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, make, vec![]);
    resolve_stack(&mut game);
    let goblin = on_battlefield(&game, "Goblin").unwrap();
    assert_eq!(game.power(goblin), 1);
    cast(&mut game, bolt, vec![Target::Object(goblin)]);
    resolve_stack(&mut game);
    assert!(on_battlefield(&game, "Goblin").is_none());
    assert_eq!(
        names(&game, game.graveyard(0)),
        vec!["Make Goblin", "Shock"]
    );
}

// ---------------------------------------------------------------------------
// Combat (CR 506–511)

/// Starts at player 0's declare attackers step.
fn combat(b: GameBuilder) -> Game {
    let game = b.start(Step::DeclareAttackers);
    assert!(matches!(
        decision(&game),
        DecisionKind::DeclareAttackers { .. }
    ));
    game
}

fn attack(game: &mut Game, attackers: Vec<ObjectId>) {
    act(game, Action::DeclareAttackers { attackers });
}

fn block(game: &mut Game, blocks: Vec<(ObjectId, ObjectId)>) {
    pass(game);
    pass(game);
    assert_eq!(game.step(), Step::DeclareBlockers);
    act(game, Action::DeclareBlockers { blocks });
}

// CR 508.1f, 510.1b: attacking taps the creature, and an unblocked creature
// deals its damage to the defending player.
#[test]
fn unblocked_attacker_damages_the_player() {
    let mut b = GameBuilder::new(vec![]);
    b.libraries(5);
    let attacker = b.battlefield(0, creature("Ogre", "{2}{R}", 3, 3, ""));
    let mut game = combat(b);
    attack(&mut game, vec![attacker]);
    assert!(game.is_tapped(attacker));
    pass_until(&mut game, Step::EndOfCombat);
    assert_eq!(game.life(1), 17);
}

// CR 702.20b: attacking doesn't cause creatures with vigilance to tap.
#[test]
fn vigilance() {
    let mut b = GameBuilder::new(vec![]);
    let attacker = b.battlefield(0, creature("Knight", "{W}", 2, 2, "Vigilance"));
    let mut game = combat(b);
    attack(&mut game, vec![attacker]);
    assert!(!game.is_tapped(attacker));
}

// CR 302.6, 702.10b, 702.3b: summoning-sick creatures can't attack unless
// they have haste, and creatures with defender can't attack.
#[test]
fn who_can_attack() {
    let mut b = GameBuilder::new(vec![]);
    let ready = b.battlefield(0, bear());
    let sick = b.battlefield(0, bear());
    let hasty = b.battlefield(0, creature("Hasty", "{R}", 1, 1, "Haste"));
    let wall = b.battlefield(0, creature("Wall", "{W}", 0, 4, "Defender"));
    b.game.objects.get_mut(&sick).unwrap().summoning_sick = true;
    b.game.objects.get_mut(&hasty).unwrap().summoning_sick = true;
    let mut game = combat(b);
    let DecisionKind::DeclareAttackers { candidates } = decision(&game).clone() else {
        unreachable!()
    };
    assert_eq!(candidates, vec![ready, hasty]);
    assert!(
        game.clone()
            .apply(
                0,
                Action::DeclareAttackers {
                    attackers: vec![sick]
                }
            )
            .is_err()
    );
    assert!(
        game.clone()
            .apply(
                0,
                Action::DeclareAttackers {
                    attackers: vec![wall]
                }
            )
            .is_err()
    );
    attack(&mut game, vec![ready, hasty]);
}

// CR 508.8: with no attackers, the declare blockers and combat damage steps
// are skipped.
#[test]
fn no_attackers_skips_blockers_and_damage() {
    let mut b = GameBuilder::new(vec![]);
    let creature = b.battlefield(0, bear());
    let mut game = combat(b);
    attack(&mut game, vec![]);
    pass(&mut game);
    pass(&mut game);
    assert_eq!(game.step(), Step::EndOfCombat);
    assert!(!stepped_through(&game, Step::DeclareBlockers));
    let _ = creature;
}

// CR 702.9b, 702.17b: a creature with flying can be blocked only by
// creatures with flying or reach.
#[test]
fn flying_and_reach() {
    let mut b = GameBuilder::new(vec![]);
    let bird = b.battlefield(0, creature("Bird", "{W}", 1, 1, "Flying"));
    let ground = b.battlefield(1, bear());
    let spider = b.battlefield(1, creature("Spider", "{G}", 1, 3, "Reach"));
    let mut game = combat(b);
    attack(&mut game, vec![bird]);
    pass(&mut game);
    pass(&mut game);
    let DecisionKind::DeclareBlockers { candidates } = decision(&game).clone() else {
        panic!("expected blockers");
    };
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].blocker, spider);
    assert!(
        game.clone()
            .apply(
                1,
                Action::DeclareBlockers {
                    blocks: vec![(ground, bird)]
                }
            )
            .is_err()
    );
    act(
        &mut game,
        Action::DeclareBlockers {
            blocks: vec![(spider, bird)],
        },
    );
}

// CR 702.111b: a creature with menace can't be blocked except by two or more
// creatures.
#[test]
fn menace() {
    let mut b = GameBuilder::new(vec![]);
    let brute = b.battlefield(0, creature("Brute", "{R}", 3, 3, "Menace"));
    let x = b.battlefield(1, bear());
    let y = b.battlefield(1, bear());
    let mut game = combat(b);
    attack(&mut game, vec![brute]);
    pass(&mut game);
    pass(&mut game);
    assert!(
        game.clone()
            .apply(
                1,
                Action::DeclareBlockers {
                    blocks: vec![(x, brute)]
                }
            )
            .is_err()
    );
    assert!(!game.legal_actions().contains(&Action::DeclareBlockers {
        blocks: vec![(x, brute)]
    }));
    act(
        &mut game,
        Action::DeclareBlockers {
            blocks: vec![(x, brute), (y, brute)],
        },
    );
}

// CR 510.1c: a creature blocked by two creatures divides its damage among
// them as its controller chooses.
#[test]
fn damage_divided_among_blockers() {
    let mut b = GameBuilder::new(vec![]);
    let giant = b.battlefield(0, creature("Giant", "{4}", 4, 6, ""));
    let x = b.battlefield(1, bear());
    let y = b.battlefield(1, bear());
    let mut game = combat(b);
    attack(&mut game, vec![giant]);
    block(&mut game, vec![(x, giant), (y, giant)]);
    pass(&mut game);
    pass(&mut game);
    let DecisionKind::AssignCombatDamage {
        power, trample_to, ..
    } = decision(&game).clone()
    else {
        panic!("expected a damage assignment");
    };
    assert_eq!((power, trample_to), (4, None));
    assert!(
        game.clone()
            .apply(
                0,
                Action::AssignCombatDamage {
                    assignments: vec![(Target::Object(x), 3)]
                }
            )
            .is_err()
    );
    act(
        &mut game,
        Action::AssignCombatDamage {
            assignments: vec![(Target::Object(x), 2), (Target::Object(y), 2)],
        },
    );
    assert!(game.battlefield().iter().all(|id| *id == giant));
    assert_eq!(game.damage(giant), 4);
}

// CR 702.19b: a trampler assigns lethal damage to its blockers before any
// excess goes to the player.
#[test]
fn trample() {
    let mut b = GameBuilder::new(vec![]);
    let wurm = b.battlefield(0, creature("Wurm", "{5}", 5, 5, "Trample"));
    let chump = b.battlefield(1, bear());
    let mut game = combat(b);
    attack(&mut game, vec![wurm]);
    block(&mut game, vec![(chump, wurm)]);
    pass(&mut game);
    pass(&mut game);
    assert_eq!(
        decision(&game),
        &DecisionKind::AssignCombatDamage {
            attacker: wurm,
            power: 5,
            blockers: vec![(chump, 2)],
            trample_to: Some(1),
        }
    );
    let too_little = Action::AssignCombatDamage {
        assignments: vec![(Target::Object(chump), 1), (Target::Player(1), 4)],
    };
    assert!(game.clone().apply(0, too_little).is_err());
    act(
        &mut game,
        Action::AssignCombatDamage {
            assignments: vec![(Target::Object(chump), 2), (Target::Player(1), 3)],
        },
    );
    assert_eq!(game.life(1), 17);
    assert!(!game.battlefield().contains(&chump));
}

// CR 702.2c: with deathtouch, 1 damage counts as lethal when assigning
// trample damage.
#[test]
fn deathtouch_trample() {
    let mut b = GameBuilder::new(vec![]);
    let snake = b.battlefield(0, creature("Snake", "{3}", 4, 4, "Deathtouch, trample"));
    let wall = b.battlefield(1, creature("Wall", "{3}", 0, 10, ""));
    let mut game = combat(b);
    attack(&mut game, vec![snake]);
    block(&mut game, vec![(wall, snake)]);
    pass(&mut game);
    pass(&mut game);
    act(
        &mut game,
        Action::AssignCombatDamage {
            assignments: vec![(Target::Object(wall), 1), (Target::Player(1), 3)],
        },
    );
    assert_eq!(game.life(1), 17);
    // CR 704.5h: any deathtouch damage destroys the creature.
    assert!(!game.battlefield().contains(&wall));
}

// CR 704.5h: a creature dealt damage by a source with deathtouch is
// destroyed.
#[test]
fn deathtouch_blocker() {
    let mut b = GameBuilder::new(vec![]);
    let giant = b.battlefield(0, creature("Giant", "{4}", 5, 5, ""));
    let rat = b.battlefield(1, creature("Rat", "{B}", 1, 1, "Deathtouch"));
    let mut game = combat(b);
    attack(&mut game, vec![giant]);
    block(&mut game, vec![(rat, giant)]);
    pass_until(&mut game, Step::EndOfCombat);
    assert!(game.battlefield().is_empty());
}

// CR 510.4, 702.7b: creatures with first strike deal combat damage in an
// earlier step, so a blocker they kill deals none.
#[test]
fn first_strike() {
    let mut b = GameBuilder::new(vec![]);
    let knight = b.battlefield(0, creature("Knight", "{W}", 2, 2, "First strike"));
    let blocker = b.battlefield(1, bear());
    let mut game = combat(b);
    attack(&mut game, vec![knight]);
    block(&mut game, vec![(blocker, knight)]);
    pass_until(&mut game, Step::EndOfCombat);
    assert!(stepped_through(&game, Step::FirstStrikeDamage));
    assert_eq!(game.battlefield(), &[knight]);
    assert_eq!(game.damage(knight), 0);
}

// CR 702.4b: a creature with double strike deals combat damage in both
// combat damage steps.
#[test]
fn double_strike() {
    let mut b = GameBuilder::new(vec![]);
    let duelist = b.battlefield(0, creature("Duelist", "{R}", 2, 2, "Double strike"));
    let mut game = combat(b);
    attack(&mut game, vec![duelist]);
    pass_until(&mut game, Step::EndOfCombat);
    assert_eq!(game.life(1), 16);
}

// CR 702.15b: damage dealt by a source with lifelink causes its controller
// to gain that much life.
#[test]
fn lifelink() {
    let mut b = GameBuilder::new(vec![]);
    let cleric = b.battlefield(0, creature("Cleric", "{W}", 3, 3, "Lifelink"));
    let mut game = combat(b);
    attack(&mut game, vec![cleric]);
    pass_until(&mut game, Step::EndOfCombat);
    assert_eq!((game.life(0), game.life(1)), (23, 17));
}

// CR 509.1h, 510.1c: a blocked creature stays blocked if its blocker is
// removed, and without trample deals no combat damage.
#[test]
fn blocked_creature_stays_blocked() {
    let mut b = GameBuilder::new(vec![shock()]);
    b.battlefield(0, basic("Mountain"));
    let ogre = b.battlefield(0, creature("Ogre", "{2}{R}", 3, 3, ""));
    let blocker = b.battlefield(1, bear());
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let mut game = combat(b);
    attack(&mut game, vec![ogre]);
    block(&mut game, vec![(blocker, ogre)]);
    cast(&mut game, bolt, vec![Target::Object(blocker)]);
    resolve_stack(&mut game);
    pass_until(&mut game, Step::EndOfCombat);
    assert_eq!(game.life(1), 20);
}

// ---------------------------------------------------------------------------
// Triggered abilities (CR 603)

fn gain_on_enter() -> CardScript {
    script(
        r#"
        name = "Visionary"
        [[triggered]]
        on = { event = "enters", object = "self" }
        effect = "draw"
        "#,
    )
}

// CR 603.6a, 603.3: an enters-the-battlefield ability triggers and goes on
// the stack the next time a player would receive priority.
#[test]
fn enters_trigger() {
    let mut b = GameBuilder::new(vec![gain_on_enter()]);
    b.libraries(3);
    b.battlefield(0, basic("Forest"));
    let card = b.hand(
        0,
        creature(
            "Visionary",
            "{G}",
            1,
            1,
            "When this creature enters, draw a card.",
        ),
    );
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, card, vec![]);
    pass(&mut game);
    pass(&mut game);
    assert_eq!(game.stack().len(), 1);
    assert!(
        game.events()
            .iter()
            .any(|e| matches!(e, Event::AbilityTriggered { .. }))
    );
    resolve_stack(&mut game);
    assert_eq!(game.hand(0).len(), 1);
}

// CR 603.10a, 700.4: a "dies" ability triggers from the graveyard, looking
// back at the creature as it was on the battlefield.
#[test]
fn dies_trigger() {
    let martyr = script(
        r#"
        name = "Martyr"
        [[triggered]]
        on = { event = "dies", object = "self" }
        effect = "gain_life"
        amount = 3
        "#,
    );
    let mut b = GameBuilder::new(vec![martyr, shock()]);
    b.battlefield(0, basic("Mountain"));
    let target = b.battlefield(
        1,
        creature(
            "Martyr",
            "{W}",
            1,
            1,
            "When this creature dies, you gain 3 life.",
        ),
    );
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, bolt, vec![Target::Object(target)]);
    resolve_stack(&mut game);
    assert_eq!(game.life(1), 23);
}

// CR 603.10a, 700.4: "whenever another creature you control dies" watches
// other creatures from the battlefield.
#[test]
fn another_creature_dies_trigger() {
    let artist = script(
        r#"
        name = "Mourner"
        [[triggered]]
        on = { event = "dies", object = { each = "creature", controller = "you", other = true } }
        effect = "lose_life"
        amount = 1
        player = "each_opponent"
        "#,
    );
    let mut b = GameBuilder::new(vec![artist, shock()]);
    b.battlefield(0, basic("Mountain"));
    b.battlefield(1, creature("Mourner", "{B}", 1, 3, ""));
    let victim = b.battlefield(1, bear());
    let bolt = b.hand(0, instant("Shock", "{R}"));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, bolt, vec![Target::Object(victim)]);
    resolve_stack(&mut game);
    assert_eq!(game.life(0), 19);
}

// CR 500.6, 503.1a: "at the beginning of your upkeep" triggers as the step
// begins and goes on the stack before the active player gets priority.
#[test]
fn upkeep_trigger() {
    let tithe = script(
        r#"
        name = "Tithe"
        [[triggered]]
        on = { event = "beginning_of_step", step = "upkeep", player = "you" }
        effect = "lose_life"
        amount = 1
        "#,
    );
    let mut b = GameBuilder::new(vec![tithe]);
    b.libraries(3);
    b.battlefield(0, card_named("Tithe"));
    b.battlefield(1, card_named("Tithe"));
    let mut game = b.start(Step::Upkeep);
    assert_eq!(game.stack().len(), 1);
    resolve_stack(&mut game);
    assert_eq!((game.life(0), game.life(1)), (19, 20));
}

fn card_named(name: &str) -> Card {
    crate::card::testing::card(name, "1", "{1}", "Enchantment", "")
}

// CR 603.3b: triggers are put on the stack in APNAP order, so the
// nonactive player's resolve first.
#[test]
fn apnap_order() {
    let blessing = script(
        r#"
        name = "Blessing"
        [[triggered]]
        on = { event = "beginning_of_step", step = "upkeep", player = "each" }
        effect = "gain_life"
        amount = 1
        "#,
    );
    let mut b = GameBuilder::new(vec![blessing]);
    b.libraries(3);
    b.battlefield(0, card_named("Blessing"));
    b.battlefield(1, card_named("Blessing"));
    let mut game = b.start(Step::Upkeep);
    resolve_stack(&mut game);
    let gains: Vec<usize> = game
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::LifeGained { player, .. } => Some(*player),
            _ => None,
        })
        .collect();
    assert_eq!(gains, vec![1, 0]);
}

// CR 603.3b: a player puts their own simultaneous triggers on the stack in
// the order they choose.
#[test]
fn player_orders_their_triggers() {
    let first = script(
        r#"
        name = "Gain Two"
        [[triggered]]
        on = { event = "beginning_of_step", step = "upkeep", player = "you" }
        effect = "gain_life"
        amount = 2
        "#,
    );
    let second = script(
        r#"
        name = "Lose One"
        [[triggered]]
        on = { event = "beginning_of_step", step = "upkeep", player = "you" }
        effect = "lose_life"
        amount = 1
        "#,
    );
    let mut b = GameBuilder::new(vec![first, second]);
    b.libraries(3);
    b.battlefield(0, card_named("Gain Two"));
    b.battlefield(0, card_named("Lose One"));
    let mut game = b.start(Step::Upkeep);
    let DecisionKind::OrderTriggers { triggers } = decision(&game).clone() else {
        panic!("expected to order triggers");
    };
    assert_eq!(triggers.len(), 2);
    // Put "Gain Two" on the stack last, so it resolves first.
    let gain = triggers.iter().position(|t| t.name == "Gain Two").unwrap();
    act(
        &mut game,
        Action::OrderTriggers {
            order: vec![1 - gain, gain],
        },
    );
    assert_eq!(game.stack().len(), 2);
    resolve_stack(&mut game);
    let first_change = game
        .events()
        .iter()
        .find(|e| matches!(e, Event::LifeGained { .. } | Event::LifeLost { .. }))
        .unwrap();
    assert!(matches!(first_change, Event::LifeGained { amount: 2, .. }));
}

// CR 603.3d: a triggered ability's targets are chosen as it's put on the
// stack; with no legal target, it's removed.
#[test]
fn triggered_ability_targets() {
    let pinger = script(
        r#"
        name = "Pinger"
        [[triggered]]
        on = { event = "enters", object = "self" }
        effect = "deal_damage"
        amount = 1
        target = { kind = "player" }
        "#,
    );
    let assassin = script(
        r#"
        name = "Assassin"
        [[triggered]]
        on = { event = "enters", object = "self" }
        effect = "destroy"
        target = { kind = "creature", controller = "opponent" }
        "#,
    );
    let mut b = GameBuilder::new(vec![pinger, assassin]);
    b.battlefield(0, basic("Swamp"));
    b.battlefield(0, basic("Swamp"));
    let ping = b.hand(0, creature("Pinger", "{B}", 1, 1, ""));
    let kill = b.hand(0, creature("Assassin", "{B}", 1, 1, ""));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, ping, vec![]);
    pass(&mut game);
    pass(&mut game);
    let DecisionKind::ChooseTargets { options, .. } = decision(&game).clone() else {
        panic!("expected to choose targets");
    };
    assert_eq!(
        options[0].targets,
        vec![vec![Target::Player(0), Target::Player(1)]]
    );
    act(
        &mut game,
        Action::ChooseTargets {
            mode: None,
            targets: vec![Target::Player(1)],
        },
    );
    resolve_stack(&mut game);
    assert_eq!(game.life(1), 19);

    // The opponent controls no creature: the trigger is removed.
    cast(&mut game, kill, vec![]);
    pass(&mut game);
    pass(&mut game);
    assert!(game.stack().is_empty());
    assert_eq!(decision(&game), &DecisionKind::Priority);
}

// CR 601.2i: "when you cast this spell" triggers on casting and resolves
// before the spell.
#[test]
fn cast_trigger() {
    let titan = script(
        r#"
        name = "Titan"
        [[triggered]]
        on = { event = "cast", object = "self" }
        effect = "gain_life"
        amount = 5
        "#,
    );
    let mut b = GameBuilder::new(vec![titan]);
    b.battlefield(0, basic("Forest"));
    let card = b.hand(0, creature("Titan", "{G}", 6, 6, ""));
    let mut game = b.start(Step::PrecombatMain);
    cast(&mut game, card, vec![]);
    assert_eq!(game.stack().len(), 2);
    pass(&mut game);
    pass(&mut game);
    assert_eq!(game.life(0), 25);
    assert!(on_battlefield(&game, "Titan").is_none());
    resolve_stack(&mut game);
    assert!(on_battlefield(&game, "Titan").is_some());
}

// CR 508.3a: "whenever this creature attacks" triggers when it's declared as
// an attacker.
#[test]
fn attack_trigger() {
    let raider = script(
        r#"
        name = "Raider"
        [[triggered]]
        on = { event = "attacks", object = "self" }
        effect = "pump"
        target = "this"
        power = 2
        "#,
    );
    let mut b = GameBuilder::new(vec![raider]);
    let attacker = b.battlefield(0, creature("Raider", "{R}", 1, 1, ""));
    let mut game = combat(b);
    attack(&mut game, vec![attacker]);
    assert_eq!(game.stack().len(), 1);
    pass_until(&mut game, Step::EndOfCombat);
    assert_eq!(game.life(1), 17);
}

// ---------------------------------------------------------------------------
// Activated and static abilities (CR 602, 604, 611)

fn firebreather() -> CardScript {
    script(
        r#"
        name = "Firebreather"
        [[activated]]
        cost = { mana = "{R}" }
        effect = "pump"
        target = "this"
        power = 1
        "#,
    )
}

// CR 602.2, 514.2: an activated ability uses the stack; its "until end of
// turn" effect ends in the cleanup step.
#[test]
fn activated_pump_until_end_of_turn() {
    let mut b = GameBuilder::new(vec![firebreather()]);
    b.libraries(3);
    b.battlefield(0, basic("Mountain"));
    let dragon = b.battlefield(
        0,
        creature(
            "Firebreather",
            "{R}",
            1,
            1,
            "{R}: This creature gets +1/+0 until end of turn.",
        ),
    );
    let mut game = b.start(Step::PrecombatMain);
    activate(&mut game, dragon, vec![]);
    assert_eq!(game.stack().len(), 1);
    assert_eq!(game.power(dragon), 1);
    resolve_stack(&mut game);
    assert_eq!(game.power(dragon), 2);
    finish_turn(&mut game);
    assert_eq!(game.power(dragon), 1);
}

// CR 302.6: a creature's {T} abilities can't be activated while it's
// summoning sick, unless it has haste.
#[test]
fn tap_abilities_and_summoning_sickness() {
    let pinger = script(
        r#"
        name = "Prodigy"
        [[activated]]
        cost = { tap = true }
        effect = "deal_damage"
        amount = 1
        target = { kind = "player" }
        "#,
    );
    let mut b = GameBuilder::new(vec![pinger]);
    let sick = b.battlefield(0, creature("Prodigy", "{U}", 1, 1, ""));
    let hasty = b.battlefield(0, creature("Prodigy", "{U}", 1, 1, "Haste"));
    b.game.objects.get_mut(&sick).unwrap().summoning_sick = true;
    b.game.objects.get_mut(&hasty).unwrap().summoning_sick = true;
    let mut game = b.start(Step::PrecombatMain);
    assert!(
        !game
            .legal_actions()
            .iter()
            .any(|a| matches!(a, Action::ActivateAbility { source, .. } if *source == sick))
    );
    activate(&mut game, hasty, vec![Target::Player(1)]);
    assert!(game.is_tapped(hasty));
    resolve_stack(&mut game);
    assert_eq!(game.life(1), 19);
}

// CR 118.3, 602.2b: sacrificing the source is part of the cost, so the
// ability resolves even though its source is gone.
#[test]
fn sacrifice_cost() {
    let salve = script(
        r#"
        name = "Salve"
        [[activated]]
        cost = { sacrifice_self = true }
        effect = "gain_life"
        amount = 3
        "#,
    );
    let mut b = GameBuilder::new(vec![salve]);
    let source = b.battlefield(0, card_named("Salve"));
    let mut game = b.start(Step::PrecombatMain);
    activate(&mut game, source, vec![]);
    assert_eq!(names(&game, game.graveyard(0)), vec!["Salve"]);
    resolve_stack(&mut game);
    assert_eq!(game.life(0), 23);
}

// CR 604.1, 613.4c: a static ability changes other creatures' power and
// toughness while its source is on the battlefield.
#[test]
fn anthem() {
    let lord = script(
        r#"
        name = "Lord"
        [[static]]
        affects = { each = "creature", controller = "you", other = true }
        power = 1
        toughness = 1
        keywords = ["Flying"]
        "#,
    );
    let mut b = GameBuilder::new(vec![lord]);
    let source = b.battlefield(0, creature("Lord", "{1}{W}", 2, 2, ""));
    let mine = b.battlefield(0, bear());
    let theirs = b.battlefield(1, bear());
    let game = b.start(Step::PrecombatMain);
    assert_eq!((game.power(mine), game.toughness(mine)), (3, 3));
    assert!(game.has_keyword(mine, &crate::card::Keyword::Flying));
    assert_eq!(game.power(source), 2);
    assert_eq!(game.power(theirs), 2);
}

// ---------------------------------------------------------------------------
// Whole games

/// Plays seeded games with players who pick random legal actions.
fn random_game(seed: u64) -> Game {
    let deck: Vec<Card> = (0..20)
        .map(|_| basic("Mountain"))
        .chain((0..12).map(|_| bear()))
        .chain((0..8).map(|_| creature("Knight", "{1}{R}", 2, 1, "First strike")))
        .chain((0..8).map(|_| creature("Bird", "{1}", 1, 1, "Flying")))
        .chain((0..12).map(|_| instant("Shock", "{R}")))
        .collect();
    let scripts = ScriptPool::new([shock()]);
    let mut game = Game::new([deck.clone(), deck], &scripts, seed).unwrap();
    let mut rng = super::rng::Rng::new(seed);
    let mut decisions = 0;
    while let Some(decision) = game.pending().cloned() {
        let actions = game.legal_actions();
        assert!(!actions.is_empty(), "no legal action for {decision:?}");
        // Pass more often than not, so games move along.
        let action = if decision.kind == DecisionKind::Priority && rng.below(3) == 0 {
            Action::Pass
        } else {
            actions[rng.below(actions.len())].clone()
        };
        game.apply(decision.player, action).unwrap();
        decisions += 1;
        assert!(decisions < 20_000, "game didn't end");
    }
    game
}

#[test]
fn random_games_finish() {
    for seed in 0..30 {
        let game = random_game(seed);
        assert!(game.result().is_some());
    }
}

#[test]
fn zones_stay_consistent() {
    let game = random_game(3);
    for (id, obj) in &game.objects {
        let listed = match obj.zone {
            Zone::Library => game.players[obj.owner].library.contains(id),
            Zone::Hand => game.players[obj.owner].hand.contains(id),
            Zone::Graveyard => game.players[obj.owner].graveyard.contains(id),
            Zone::Battlefield => game.battlefield.contains(id),
            Zone::Exile => game.exile.contains(id),
            Zone::Stack => game.stack.iter().any(|s| s.id == *id),
        };
        assert!(listed, "{id} isn't in its zone");
    }
    let cards: usize = game
        .players
        .iter()
        .map(|p| p.library.len() + p.hand.len() + p.graveyard.len())
        .sum::<usize>()
        + game.battlefield.len()
        + game.exile.len()
        + game.stack.len();
    assert_eq!(cards, game.objects.len());
}
