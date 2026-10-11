//! Reading an object's characteristics, and the basic operations effects and
//! rules are built from: moving objects, drawing, damage and life.

use super::Game;
use super::effect::{EachSpec, Filter, Kind, Relation, TokenSpec, printed_keywords};
use super::event::Event;
use super::mana::Mana;
use super::script::NATIVE_KEYWORDS;
use super::state::{GameObject, ObjectId, PlayerId, StackItem, StackKind, Target, Zone};
use crate::card::{CardType, Keyword, Stat, Supertype};

/// What dealt damage, as it was when it dealt it (CR 120.3, 702.2e, 702.15c).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DamageSource {
    pub id: ObjectId,
    pub controller: PlayerId,
    pub deathtouch: bool,
    pub lifelink: bool,
}

impl Game {
    /// An object as it is now, or as it last existed if it left its zone
    /// (last known information, CR 113.7a).
    pub(crate) fn obj_or_lki(&self, id: ObjectId) -> Option<&GameObject> {
        self.objects.get(&id).or_else(|| self.lki.get(&id))
    }

    /// The players in APNAP order (CR 101.4).
    pub(crate) fn apnap(&self) -> Vec<PlayerId> {
        vec![self.active, opponent(self.active)]
    }

    pub(crate) fn alloc_id(&mut self) -> ObjectId {
        self.next_id += 1;
        ObjectId(self.next_id)
    }

    pub(crate) fn is_type(&self, id: ObjectId, card_type: CardType) -> bool {
        self.obj_or_lki(id).is_some_and(|o| o.card.is(card_type))
    }

    pub(crate) fn is_legendary(&self, id: ObjectId) -> bool {
        self.obj_or_lki(id)
            .is_some_and(|o| o.card.type_line.has_supertype(Supertype::Legendary))
    }

    /// The object's natively supported keyword abilities: printed, granted
    /// until end of turn, or granted by static abilities.
    pub fn keywords(&self, id: ObjectId) -> Vec<Keyword> {
        let Some(obj) = self.obj_or_lki(id) else {
            return Vec::new();
        };
        let mut keywords: Vec<Keyword> = printed_keywords(&obj.card)
            .filter(|k| NATIVE_KEYWORDS.contains(k))
            .cloned()
            .collect();
        for effect in self.temp.iter().filter(|e| e.object == id) {
            keywords.extend(effect.keywords.iter().cloned());
        }
        keywords.extend(self.static_bonus(id).2);
        keywords.dedup();
        keywords
    }

    pub fn has_keyword(&self, id: ObjectId, keyword: &Keyword) -> bool {
        self.keywords(id).contains(keyword)
    }

    /// Power, with counters and effects applied (CR 613.4).
    pub fn power(&self, id: ObjectId) -> i32 {
        let Some(obj) = self.obj_or_lki(id) else {
            return 0;
        };
        base(&obj.card.power)
            + obj.plus_counters as i32
            + self
                .temp
                .iter()
                .filter(|e| e.object == id)
                .map(|e| e.power)
                .sum::<i32>()
            + self.static_bonus(id).0
    }

    /// Toughness, with counters and effects applied (CR 613.4).
    pub fn toughness(&self, id: ObjectId) -> i32 {
        let Some(obj) = self.obj_or_lki(id) else {
            return 0;
        };
        base(&obj.card.toughness)
            + obj.plus_counters as i32
            + self
                .temp
                .iter()
                .filter(|e| e.object == id)
                .map(|e| e.toughness)
                .sum::<i32>()
            + self.static_bonus(id).1
    }

    /// What static abilities of permanents add to `id`.
    fn static_bonus(&self, id: ObjectId) -> (i32, i32, Vec<Keyword>) {
        let mut bonus = (0, 0, Vec::new());
        if !self.battlefield.contains(&id) {
            return bonus;
        }
        for &source in &self.battlefield {
            let obj = &self.objects[&source];
            let Some(script) = &obj.script else { continue };
            for ability in &script.statics {
                if self.object_matches(
                    ability.affects.each,
                    &ability.affects.filter,
                    id,
                    obj.controller,
                    source,
                ) {
                    bonus.0 += ability.power;
                    bonus.1 += ability.toughness;
                    bonus.2.extend(ability.keywords.iter().cloned());
                }
            }
        }
        bonus
    }

    pub(crate) fn player_matches(
        &self,
        kind: Kind,
        player: PlayerId,
        controller: PlayerId,
    ) -> bool {
        match kind {
            Kind::Any | Kind::Player => true,
            Kind::Opponent => player != controller,
            _ => false,
        }
    }

    /// Whether `id`, a current object in the zone `kind` picks from, matches.
    pub(crate) fn object_matches(
        &self,
        kind: Kind,
        filter: &Filter,
        id: ObjectId,
        controller: PlayerId,
        source: ObjectId,
    ) -> bool {
        let Some(obj) = self.objects.get(&id) else {
            return false;
        };
        let in_zone = match kind.zone() {
            Zone::Stack => self
                .stack
                .iter()
                .any(|s| s.id == id && matches!(s.kind, StackKind::Spell)),
            zone => obj.zone == zone,
        };
        kind.includes_objects()
            && in_zone
            && type_matches(kind, obj)
            && filter_matches(filter, obj, controller, source)
    }

    /// Like [`object_matches`](Game::object_matches), but for an object in
    /// any zone, or one that has left its zone (CR 603.10).
    pub(crate) fn object_matches_anywhere(
        &self,
        spec: &EachSpec,
        id: ObjectId,
        controller: PlayerId,
        source: ObjectId,
    ) -> bool {
        self.obj_or_lki(id).is_some_and(|obj| {
            spec.each.includes_objects()
                && type_matches(spec.each, obj)
                && filter_matches(&spec.filter, obj, controller, source)
        })
    }

    /// Moves an object to `to` and returns its new id (CR 400.7). Cards go to
    /// their owner's hand, library or graveyard.
    pub(crate) fn move_object(&mut self, id: ObjectId, to: Zone) -> ObjectId {
        self.move_object_to(id, to, None, false)
    }

    /// Like [`move_object`](Game::move_object), but onto the battlefield or
    /// stack under `controller`, or to the bottom of a library.
    pub(crate) fn move_object_to(
        &mut self,
        id: ObjectId,
        to: Zone,
        controller: Option<PlayerId>,
        bottom: bool,
    ) -> ObjectId {
        let old = self
            .objects
            .remove(&id)
            .expect("moved an object that doesn't exist");
        let owner = old.owner;
        match old.zone {
            Zone::Library => self.players[owner].library.retain(|x| *x != id),
            Zone::Hand => self.players[owner].hand.retain(|x| *x != id),
            Zone::Graveyard => self.players[owner].graveyard.retain(|x| *x != id),
            Zone::Battlefield => self.battlefield.retain(|x| *x != id),
            Zone::Exile => self.exile.retain(|x| *x != id),
            Zone::Stack => self.stack.retain(|s| s.id != id),
        }

        let new_id = self.alloc_id();
        let controller = controller.unwrap_or(owner);
        let obj = GameObject {
            id: new_id,
            zone: to,
            controller,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            plus_counters: 0,
            summoning_sick: true,
            ..old.clone()
        };
        match to {
            Zone::Library if bottom => self.players[owner].library.insert(0, new_id),
            Zone::Library => self.players[owner].library.push(new_id),
            Zone::Hand => self.players[owner].hand.push(new_id),
            Zone::Graveyard => self.players[owner].graveyard.push(new_id),
            Zone::Battlefield => self.battlefield.push(new_id),
            Zone::Exile => self.exile.push(new_id),
            Zone::Stack => self.stack.push(StackItem {
                id: new_id,
                controller,
                kind: StackKind::Spell,
                mode: None,
                targets: Vec::new(),
            }),
        }
        let name = old.card.name.clone();
        let from = old.zone;
        self.objects.insert(new_id, obj);
        self.lki.insert(id, old);
        self.emit(Event::ZoneChanged {
            old: Some(id),
            new: new_id,
            name,
            from: Some(from),
            to,
        });
        new_id
    }

    /// CR 121.1: draw the top card of the library. Drawing from an empty
    /// library loses the game the next time state-based actions are checked
    /// (CR 704.5b).
    pub(crate) fn draw(&mut self, player: PlayerId) {
        match self.players[player].library.last() {
            None => self.players[player].drew_from_empty = true,
            Some(&top) => {
                let card = self.move_object(top, Zone::Hand);
                self.emit(Event::CardDrawn { player, card });
            }
        }
    }

    /// CR 701.8: a destroyed permanent goes to its owner's graveyard, unless
    /// it has indestructible (CR 702.12b).
    pub(crate) fn destroy(&mut self, id: ObjectId) {
        if self.battlefield.contains(&id) && !self.has_keyword(id, &Keyword::Indestructible) {
            self.move_object(id, Zone::Graveyard);
        }
    }

    /// CR 701.6a: a countered spell goes to its owner's graveyard; a
    /// countered ability ceases to exist.
    pub(crate) fn counter(&mut self, id: ObjectId) {
        let Some(spell) = self
            .stack
            .iter()
            .find(|s| s.id == id)
            .map(|s| matches!(s.kind, StackKind::Spell))
        else {
            return;
        };
        self.emit(Event::Countered { id });
        if spell {
            self.move_object(id, Zone::Graveyard);
        } else {
            self.stack.retain(|s| s.id != id);
        }
    }

    pub(crate) fn add_mana(&mut self, player: PlayerId, mana: &[Mana]) {
        for m in mana {
            self.players[player].mana.add(*m, 1);
        }
        self.emit(Event::ManaAdded {
            player,
            mana: mana.to_vec(),
        });
    }

    /// CR 111.2: a token enters the battlefield under the control of the
    /// player who created it.
    pub(crate) fn create_token(&mut self, spec: &TokenSpec, controller: PlayerId) -> ObjectId {
        let id = self.alloc_id();
        let card = spec.to_card();
        let name = card.name.clone();
        self.objects.insert(
            id,
            GameObject {
                id,
                card: card.into(),
                script: None,
                owner: controller,
                controller,
                zone: Zone::Battlefield,
                token: true,
                tapped: false,
                damage: 0,
                deathtouch_damage: false,
                plus_counters: 0,
                summoning_sick: true,
            },
        );
        self.battlefield.push(id);
        self.emit(Event::ZoneChanged {
            old: None,
            new: id,
            name,
            from: None,
            to: Zone::Battlefield,
        });
        id
    }

    pub(crate) fn damage_source(&self, id: ObjectId, controller: PlayerId) -> DamageSource {
        DamageSource {
            id,
            controller: self.obj_or_lki(id).map_or(controller, |o| o.controller),
            deathtouch: self.has_keyword(id, &Keyword::Deathtouch),
            lifelink: self.has_keyword(id, &Keyword::Lifelink),
        }
    }

    /// Deals damage and applies its results (CR 120.3).
    pub(crate) fn deal_damage(
        &mut self,
        source: &DamageSource,
        target: Target,
        amount: u32,
        combat: bool,
    ) {
        if amount == 0 {
            return;
        }
        match target {
            Target::Player(p) => {
                self.emit(Event::DamageDealt {
                    source: source.id,
                    target,
                    amount,
                    combat,
                });
                // CR 120.3a.
                self.lose_life(p, amount);
            }
            Target::Object(id) => {
                if !self.battlefield.contains(&id) || !self.is_type(id, CardType::Creature) {
                    return;
                }
                self.emit(Event::DamageDealt {
                    source: source.id,
                    target,
                    amount,
                    combat,
                });
                // CR 120.3e.
                let obj = self.objects.get_mut(&id).unwrap();
                obj.damage += amount;
                if source.deathtouch {
                    obj.deathtouch_damage = true;
                }
            }
        }
        // CR 120.3f, 702.15b.
        if source.lifelink {
            self.gain_life(source.controller, amount);
        }
    }

    pub(crate) fn gain_life(&mut self, player: PlayerId, amount: u32) {
        if amount > 0 {
            self.players[player].life += amount as i32;
            self.emit(Event::LifeGained { player, amount });
        }
    }

    pub(crate) fn lose_life(&mut self, player: PlayerId, amount: u32) {
        if amount > 0 {
            self.players[player].life -= amount as i32;
            self.emit(Event::LifeLost { player, amount });
        }
    }
}

pub(crate) fn opponent(player: PlayerId) -> PlayerId {
    1 - player
}

/// A printed power or toughness. Values with a star (CR 208.2) need a
/// characteristic-defining ability, which isn't supported yet, so count as 0.
fn base(stat: &Option<Stat>) -> i32 {
    match stat {
        Some(Stat::Fixed(n)) => *n,
        _ => 0,
    }
}

fn type_matches(kind: Kind, obj: &GameObject) -> bool {
    let card = &obj.card;
    match kind {
        Kind::Any | Kind::Creature | Kind::CreatureSpell => card.is(CardType::Creature),
        Kind::Permanent | Kind::Spell => true,
        Kind::NonlandPermanent => !card.is(CardType::Land),
        Kind::Artifact => card.is(CardType::Artifact),
        Kind::Enchantment => card.is(CardType::Enchantment),
        Kind::Land => card.is(CardType::Land),
        Kind::NoncreatureSpell => !card.is(CardType::Creature),
        Kind::Player | Kind::Opponent => false,
    }
}

fn filter_matches(
    filter: &Filter,
    obj: &GameObject,
    controller: PlayerId,
    source: ObjectId,
) -> bool {
    filter.controller.is_none_or(|r| match r {
        Relation::You => obj.controller == controller,
        Relation::Opponent => obj.controller != controller,
    }) && filter.color.is_none_or(|c| obj.card.colors.contains(&c))
        && filter.tapped.is_none_or(|t| obj.tapped == t)
        && !(filter.other && obj.id == source)
}
