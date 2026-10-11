//! Playing lands, casting spells, activating abilities and paying for them
//! (CR 305, 601, 602, 605).

use super::action::{Action, ManaActivation};
use super::effect::{Effect, TargetSpec};
use super::event::Event;
use super::mana::{Mana, ManaPool, produced};
use super::script::Cost;
use super::state::{ObjectId, PlayerId, StackItem, StackKind, Target, Zone};
use super::turn::Step;
use super::{Game, IllegalAction};
use crate::card::{CardType, Keyword, ManaCost};

/// At most this many target combinations are listed per spell or ability.
const MAX_TARGET_COMBINATIONS: usize = 256;

type Result<T> = std::result::Result<T, IllegalAction>;

fn illegal<T>(why: impl Into<String>) -> Result<T> {
    Err(IllegalAction(why.into()))
}

impl Game {
    /// Whether `player` could cast a sorcery-speed spell now (CR 117.1a,
    /// 307.1): their own main phase, with an empty stack.
    pub(crate) fn sorcery_timing(&self, player: PlayerId) -> bool {
        player == self.active
            && matches!(self.step, Step::PrecombatMain | Step::PostcombatMain)
            && self.stack.is_empty()
    }

    pub(crate) fn can_play_land(&self, player: PlayerId, card: ObjectId) -> Result<()> {
        let Some(obj) = self.objects.get(&card) else {
            return illegal("no such card");
        };
        if obj.zone != Zone::Hand || obj.owner != player {
            return illegal("the land isn't in your hand");
        }
        if !obj.card.is(CardType::Land) {
            return illegal("that card isn't a land");
        }
        // CR 305.2, 305.3, 505.6b.
        if !self.sorcery_timing(player) {
            return illegal("lands can be played only in your main phase with an empty stack");
        }
        if self.players[player].lands_played >= 1 {
            return illegal("you already played a land this turn");
        }
        Ok(())
    }

    pub(crate) fn play_land(&mut self, player: PlayerId, card: ObjectId) -> Result<()> {
        self.can_play_land(player, card)?;
        self.players[player].lands_played += 1;
        let land = self.move_object_to(card, Zone::Battlefield, Some(player), false);
        self.emit(Event::LandPlayed { player, card: land });
        Ok(())
    }

    /// The effect an object has as a spell, if it's an instant or sorcery.
    fn spell_effect(&self, card: ObjectId) -> Option<Effect> {
        self.objects.get(&card)?.script.as_ref()?.spell.clone()
    }

    pub(crate) fn can_cast(
        &self,
        player: PlayerId,
        card: ObjectId,
        mode: Option<usize>,
        targets: &[Target],
        mana: &[ManaActivation],
    ) -> Result<ManaPool> {
        let Some(obj) = self.objects.get(&card) else {
            return illegal("no such card");
        };
        if obj.zone != Zone::Hand || obj.owner != player {
            return illegal("the card isn't in your hand");
        }
        if obj.card.is(CardType::Land) {
            return illegal("lands are played, not cast (CR 305.1)");
        }
        // CR 117.1a, 304.1, 702.8a.
        let instant_speed =
            obj.card.is(CardType::Instant) || self.has_keyword(card, &Keyword::Flash);
        if !instant_speed && !self.sorcery_timing(player) {
            return illegal("this spell can be cast only in your main phase with an empty stack");
        }
        let effect = self.spell_effect(card);
        self.check_choices(effect.as_ref(), mode, targets, player, card)?;
        self.check_payment(player, &obj.card.mana_cost, mana, None)
    }

    /// CR 601.2.
    pub(crate) fn cast_spell(
        &mut self,
        player: PlayerId,
        card: ObjectId,
        mode: Option<usize>,
        targets: Vec<Target>,
        mana: Vec<ManaActivation>,
    ) -> Result<()> {
        let pool = self.can_cast(player, card, mode, &targets, &mana)?;
        let spell = self.move_object_to(card, Zone::Stack, Some(player), false);
        let item = self.stack.last_mut().unwrap();
        item.mode = mode;
        item.targets = targets;
        self.pay_mana(player, &mana, pool);
        self.emit(Event::SpellCast { player, spell });
        Ok(())
    }

    pub(crate) fn can_activate(
        &self,
        player: PlayerId,
        source: ObjectId,
        ability: usize,
        mode: Option<usize>,
        targets: &[Target],
        mana: &[ManaActivation],
    ) -> Result<ManaPool> {
        let Some(obj) = self.objects.get(&source) else {
            return illegal("no such permanent");
        };
        if obj.zone != Zone::Battlefield || obj.controller != player {
            return illegal("you don't control that permanent");
        }
        let Some(activated) = obj.script.as_ref().and_then(|s| s.activated.get(ability)) else {
            return illegal("no such ability");
        };
        // CR 602.5d.
        if activated.sorcery_speed && !self.sorcery_timing(player) {
            return illegal("this ability can be activated only as a sorcery");
        }
        self.check_cost(player, source, &activated.cost)?;
        self.check_choices(Some(&activated.effect), mode, targets, player, source)?;
        let exclude = activated.cost.tap.then_some(source);
        self.check_payment(player, &activated.cost.mana, mana, exclude)
    }

    /// CR 602.2.
    pub(crate) fn activate(
        &mut self,
        player: PlayerId,
        source: ObjectId,
        ability: usize,
        mode: Option<usize>,
        targets: Vec<Target>,
        mana: Vec<ManaActivation>,
    ) -> Result<()> {
        let pool = self.can_activate(player, source, ability, mode, &targets, &mana)?;
        let activated = self.objects[&source].script.as_ref().unwrap().activated[ability].clone();
        let id = self.alloc_id();
        self.stack.push(StackItem {
            id,
            controller: player,
            kind: StackKind::Ability {
                source,
                effect: activated.effect,
            },
            mode,
            targets,
        });
        self.pay_mana(player, &mana, pool);
        let cost = activated.cost;
        if cost.tap {
            self.objects.get_mut(&source).unwrap().tapped = true;
        }
        self.lose_life(player, cost.life);
        if cost.sacrifice_self {
            self.move_object(source, Zone::Graveyard);
        }
        self.emit(Event::AbilityActivated {
            player,
            source,
            ability: id,
        });
        Ok(())
    }

    /// Non-mana costs (CR 118.3, 119.4, 302.6).
    fn check_cost(&self, player: PlayerId, source: ObjectId, cost: &Cost) -> Result<()> {
        if cost.tap && !self.can_tap(source) {
            return illegal("that permanent can't be tapped for a cost now");
        }
        if cost.life > 0 && self.players[player].life < cost.life as i32 {
            return illegal("you don't have enough life to pay");
        }
        Ok(())
    }

    /// Checks the mode and targets chosen for a spell or ability (CR 601.2b,
    /// 601.2c, 115).
    fn check_choices(
        &self,
        effect: Option<&Effect>,
        mode: Option<usize>,
        targets: &[Target],
        controller: PlayerId,
        source: ObjectId,
    ) -> Result<()> {
        let Some(effect) = effect else {
            if mode.is_some() || !targets.is_empty() {
                return illegal("this has no modes or targets");
            }
            return Ok(());
        };
        if !effect.valid_mode(mode) {
            return illegal("invalid mode");
        }
        let specs = effect.targets(mode);
        if specs.len() != targets.len() {
            return illegal(format!("expected {} targets", specs.len()));
        }
        for (spec, target) in specs.iter().zip(targets) {
            if !self.target_legal(spec, *target, controller, source) {
                return illegal("illegal target");
            }
        }
        Ok(())
    }

    /// Whether `target` is a legal choice for `spec` (CR 115).
    pub(crate) fn target_legal(
        &self,
        spec: &TargetSpec,
        target: Target,
        controller: PlayerId,
        source: ObjectId,
    ) -> bool {
        match target {
            Target::Player(p) => {
                p < self.players.len() && self.player_matches(spec.kind, p, controller)
            }
            Target::Object(id) => {
                // A spell can't target itself (CR 115.5): it isn't on the
                // stack yet while its targets are chosen.
                self.object_matches(spec.kind, &spec.filter, id, controller, source)
                    && (self.objects[&id].zone != Zone::Battlefield
                        // CR 702.18a.
                        || !self.has_keyword(id, &Keyword::Shroud)
                            // CR 702.11b.
                            && (self.objects[&id].controller == controller
                                || !self.has_keyword(id, &Keyword::Hexproof)))
            }
        }
    }

    pub(crate) fn legal_targets(
        &self,
        spec: &TargetSpec,
        controller: PlayerId,
        source: ObjectId,
    ) -> Vec<Target> {
        let mut candidates: Vec<Target> = (0..self.players.len()).map(Target::Player).collect();
        candidates.extend(self.battlefield.iter().map(|id| Target::Object(*id)));
        candidates.extend(self.stack.iter().map(|s| Target::Object(s.id)));
        candidates
            .into_iter()
            .filter(|t| self.target_legal(spec, *t, controller, source))
            .collect()
    }

    /// Every legal (mode, targets) choice for `effect`, up to a limit.
    pub(crate) fn choices(
        &self,
        effect: Option<&Effect>,
        controller: PlayerId,
        source: ObjectId,
    ) -> Vec<(Option<usize>, Vec<Target>)> {
        let Some(effect) = effect else {
            return vec![(None, vec![])];
        };
        let modes: Vec<Option<usize>> = match effect.modes() {
            Some(n) => (0..n).map(Some).collect(),
            None => vec![None],
        };
        let mut out = Vec::new();
        for mode in modes {
            let options: Vec<Vec<Target>> = effect
                .targets(mode)
                .iter()
                .map(|spec| self.legal_targets(spec, controller, source))
                .collect();
            for targets in combinations(&options, MAX_TARGET_COMBINATIONS) {
                out.push((mode, targets));
            }
        }
        out
    }

    /// Whether a permanent can be tapped for a cost now: untapped, and not a
    /// creature with summoning sickness (CR 302.6, 702.10c).
    pub(crate) fn can_tap(&self, id: ObjectId) -> bool {
        let Some(obj) = self.objects.get(&id) else {
            return false;
        };
        obj.zone == Zone::Battlefield
            && !obj.tapped
            && !(obj.summoning_sick
                && obj.card.is(CardType::Creature)
                && !self.has_keyword(id, &Keyword::Haste))
    }

    /// What each of an object's "{T}: Add ..." abilities can produce: the
    /// intrinsic abilities of basic land types (CR 305.6), then scripted ones.
    pub(crate) fn mana_options(&self, id: ObjectId) -> Vec<Vec<Mana>> {
        let Some(obj) = self.objects.get(&id) else {
            return Vec::new();
        };
        let mut options = Vec::new();
        if obj.card.is(CardType::Land) {
            for (subtype, mana) in [
                ("Plains", Mana::White),
                ("Island", Mana::Blue),
                ("Swamp", Mana::Black),
                ("Mountain", Mana::Red),
                ("Forest", Mana::Green),
            ] {
                if obj.card.type_line.has_subtype(subtype) {
                    options.push(vec![mana]);
                }
            }
        }
        if let Some(script) = &obj.script {
            for ability in &script.mana {
                options.extend(ability.produces.iter().filter_map(produced));
            }
        }
        options
    }

    fn check_mana_activation(&self, player: PlayerId, activation: &ManaActivation) -> Result<()> {
        let Some(obj) = self.objects.get(&activation.source) else {
            return illegal("no such mana source");
        };
        if obj.controller != player || obj.zone != Zone::Battlefield {
            return illegal("you don't control that mana source");
        }
        if !self.can_tap(activation.source) {
            return illegal("that mana source can't be tapped now");
        }
        if !self
            .mana_options(activation.source)
            .contains(&activation.mana)
        {
            return illegal("that source can't produce that mana");
        }
        Ok(())
    }

    pub(crate) fn activate_mana(
        &mut self,
        player: PlayerId,
        activation: ManaActivation,
    ) -> Result<()> {
        self.check_mana_activation(player, &activation)?;
        self.objects.get_mut(&activation.source).unwrap().tapped = true;
        self.add_mana(player, &activation.mana);
        Ok(())
    }

    /// Checks that activating `mana` and then paying `cost` from the pool
    /// works (CR 601.2g, 601.2h), and returns the pool left afterwards.
    /// `exclude` is a permanent tapped for another part of the cost.
    fn check_payment(
        &self,
        player: PlayerId,
        cost: &ManaCost,
        mana: &[ManaActivation],
        exclude: Option<ObjectId>,
    ) -> Result<ManaPool> {
        let mut pool = self.players[player].mana;
        for (i, activation) in mana.iter().enumerate() {
            self.check_mana_activation(player, activation)?;
            if Some(activation.source) == exclude
                || mana[..i].iter().any(|a| a.source == activation.source)
            {
                return illegal("a mana source is tapped twice");
            }
            for m in &activation.mana {
                pool.add(*m, 1);
            }
        }
        match pool.pay(cost) {
            Some(left) => Ok(left),
            None => illegal(format!("can't pay {cost}")),
        }
    }

    fn pay_mana(&mut self, player: PlayerId, mana: &[ManaActivation], left: ManaPool) {
        for activation in mana {
            self.objects.get_mut(&activation.source).unwrap().tapped = true;
            self.add_mana(player, &activation.mana);
        }
        self.players[player].mana = left;
    }

    /// A smallest set of mana abilities that, with the mana already in the
    /// pool, pays `cost`.
    pub(crate) fn find_payment(
        &self,
        player: PlayerId,
        cost: &ManaCost,
        exclude: Option<ObjectId>,
    ) -> Option<Vec<ManaActivation>> {
        let pool = self.players[player].mana;
        if pool.pay(cost).is_some() {
            return Some(Vec::new());
        }
        // Sources with the same options are interchangeable, so group them.
        let mut groups: Vec<(Vec<Vec<Mana>>, Vec<ObjectId>)> = Vec::new();
        for &id in &self.battlefield {
            if self.objects[&id].controller != player || Some(id) == exclude || !self.can_tap(id) {
                continue;
            }
            let options = self.mana_options(id);
            if options.is_empty() {
                continue;
            }
            match groups.iter_mut().find(|(o, _)| *o == options) {
                Some((_, ids)) => ids.push(id),
                None => groups.push((options, vec![id])),
            }
        }
        let available: usize = groups.iter().map(|(_, ids)| ids.len()).sum();
        for count in 1..=available {
            let mut chosen = Vec::new();
            if search(&groups, 0, count, pool, cost, &mut chosen) {
                return Some(chosen);
            }
        }
        None
    }

    /// Every action the deciding player can take at priority (CR 117.1).
    pub(crate) fn priority_actions(&self, player: PlayerId) -> Vec<Action> {
        let mut actions = vec![Action::Pass];
        let hand = self.players[player].hand.clone();
        for &card in &hand {
            if self.can_play_land(player, card).is_ok() {
                actions.push(Action::PlayLand { card });
            }
        }
        for &card in &hand {
            let obj = &self.objects[&card];
            if obj.card.is(CardType::Land) {
                continue;
            }
            let instant_speed =
                obj.card.is(CardType::Instant) || self.has_keyword(card, &Keyword::Flash);
            if !instant_speed && !self.sorcery_timing(player) {
                continue;
            }
            let Some(mana) = self.find_payment(player, &obj.card.mana_cost, None) else {
                continue;
            };
            for (mode, targets) in self.choices(self.spell_effect(card).as_ref(), player, card) {
                actions.push(Action::CastSpell {
                    card,
                    mode,
                    targets,
                    mana: mana.clone(),
                });
            }
        }
        for &source in &self.battlefield {
            let obj = &self.objects[&source];
            if obj.controller != player {
                continue;
            }
            let Some(script) = &obj.script else { continue };
            for (ability, activated) in script.activated.iter().enumerate() {
                if activated.sorcery_speed && !self.sorcery_timing(player)
                    || self.check_cost(player, source, &activated.cost).is_err()
                {
                    continue;
                }
                let exclude = activated.cost.tap.then_some(source);
                let Some(mana) = self.find_payment(player, &activated.cost.mana, exclude) else {
                    continue;
                };
                for (mode, targets) in self.choices(Some(&activated.effect), player, source) {
                    actions.push(Action::ActivateAbility {
                        source,
                        ability,
                        mode,
                        targets,
                        mana: mana.clone(),
                    });
                }
            }
        }
        for &source in &self.battlefield {
            if self.objects[&source].controller != player || !self.can_tap(source) {
                continue;
            }
            for mana in self.mana_options(source) {
                actions.push(Action::ActivateManaAbility(ManaActivation { source, mana }));
            }
        }
        actions
    }
}

/// Chooses exactly `count` more sources from `groups[i..]` so that the pool
/// pays `cost`.
fn search(
    groups: &[(Vec<Vec<Mana>>, Vec<ObjectId>)],
    i: usize,
    count: usize,
    pool: ManaPool,
    cost: &ManaCost,
    chosen: &mut Vec<ManaActivation>,
) -> bool {
    if count == 0 {
        return pool.pay(cost).is_some();
    }
    let Some((options, ids)) = groups.get(i) else {
        return false;
    };
    if options.len() == 1 {
        // Take `n` of these identical single-option sources.
        for n in (0..=count.min(ids.len())).rev() {
            let mut p = pool;
            for _ in 0..n {
                for m in &options[0] {
                    p.add(*m, 1);
                }
            }
            let before = chosen.len();
            chosen.extend(ids[..n].iter().map(|&source| ManaActivation {
                source,
                mana: options[0].clone(),
            }));
            if search(groups, i + 1, count - n, p, cost, chosen) {
                return true;
            }
            chosen.truncate(before);
        }
        false
    } else {
        // Sources with a choice of mana: decide each one separately.
        search_each(options, ids, 0, groups, i, count, pool, cost, chosen)
    }
}

#[allow(clippy::too_many_arguments)]
fn search_each(
    options: &[Vec<Mana>],
    ids: &[ObjectId],
    j: usize,
    groups: &[(Vec<Vec<Mana>>, Vec<ObjectId>)],
    i: usize,
    count: usize,
    pool: ManaPool,
    cost: &ManaCost,
    chosen: &mut Vec<ManaActivation>,
) -> bool {
    if count == 0 || j == ids.len() {
        return search(groups, i + 1, count, pool, cost, chosen);
    }
    for option in options {
        let mut p = pool;
        for m in option {
            p.add(*m, 1);
        }
        chosen.push(ManaActivation {
            source: ids[j],
            mana: option.clone(),
        });
        if search_each(options, ids, j + 1, groups, i, count - 1, p, cost, chosen) {
            return true;
        }
        chosen.pop();
    }
    search_each(options, ids, j + 1, groups, i, count, pool, cost, chosen)
}

/// The cartesian product of `options`, up to `limit` combinations.
fn combinations<T: Clone>(options: &[Vec<T>], limit: usize) -> Vec<Vec<T>> {
    let mut out: Vec<Vec<T>> = vec![Vec::new()];
    for choices in options {
        let mut next = Vec::new();
        for prefix in &out {
            for choice in choices {
                if next.len() == limit {
                    break;
                }
                let mut combo = prefix.clone();
                combo.push(choice.clone());
                next.push(combo);
            }
        }
        out = next;
    }
    out
}
