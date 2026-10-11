//! What one player can see of the game (CR 400.2): everything but the
//! cards in libraries and in the opponent's hand.

use serde::{Deserialize, Serialize};

use super::Game;
use super::action::Decision;
use super::mana::ManaPool;
use super::state::{GameResult, ObjectId, PlayerId, StackKind, Target};
use super::turn::Step;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub viewer: PlayerId,
    pub turn: u32,
    pub active_player: PlayerId,
    pub step: Step,
    pub players: Vec<PlayerView>,
    pub battlefield: Vec<ObjectView>,
    /// Top last.
    pub stack: Vec<StackView>,
    pub exile: Vec<ObjectView>,
    pub pending: Option<Decision>,
    pub result: Option<GameResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerView {
    pub life: i32,
    pub poison: u32,
    pub library_size: usize,
    pub hand_size: usize,
    /// Only the viewer's own hand.
    pub hand: Option<Vec<ObjectView>>,
    /// Top last.
    pub graveyard: Vec<ObjectView>,
    pub mana: ManaPool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectView {
    pub id: ObjectId,
    pub name: String,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub type_line: String,
    pub mana_cost: String,
    pub oracle_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<i32>,
    pub tapped: bool,
    pub damage: u32,
    pub counters: u32,
    pub token: bool,
    pub summoning_sick: bool,
    pub attacking: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackView {
    pub id: ObjectId,
    pub controller: PlayerId,
    /// The spell's card, or the ability's source.
    pub name: String,
    pub is_ability: bool,
    pub targets: Vec<Target>,
}

impl Game {
    /// What `viewer` can see.
    pub fn view(&self, viewer: PlayerId) -> View {
        let players = self
            .players
            .iter()
            .enumerate()
            .map(|(p, player)| PlayerView {
                life: player.life,
                poison: player.poison,
                library_size: player.library.len(),
                hand_size: player.hand.len(),
                hand: (p == viewer)
                    .then(|| player.hand.iter().map(|id| self.object_view(*id)).collect()),
                graveyard: player
                    .graveyard
                    .iter()
                    .map(|id| self.object_view(*id))
                    .collect(),
                mana: player.mana,
            })
            .collect();
        View {
            viewer,
            turn: self.turn,
            active_player: self.active,
            step: self.step,
            players,
            battlefield: self
                .battlefield
                .iter()
                .map(|id| self.object_view(*id))
                .collect(),
            stack: self
                .stack
                .iter()
                .map(|s| StackView {
                    id: s.id,
                    controller: s.controller,
                    name: match &s.kind {
                        StackKind::Spell => self.objects[&s.id].name().to_string(),
                        StackKind::Ability { source, .. } => self
                            .obj_or_lki(*source)
                            .map_or(String::new(), |o| o.name().to_string()),
                    },
                    is_ability: matches!(s.kind, StackKind::Ability { .. }),
                    targets: s.targets.clone(),
                })
                .collect(),
            exile: self.exile.iter().map(|id| self.object_view(*id)).collect(),
            pending: self.pending.clone(),
            result: self.result,
        }
    }

    pub fn object_view(&self, id: ObjectId) -> ObjectView {
        let obj = self.obj_or_lki(id).expect("no such object");
        let creature = obj.card.is(crate::card::CardType::Creature);
        let blocks = self.blocks();
        ObjectView {
            id,
            name: obj.card.name.clone(),
            owner: obj.owner,
            controller: obj.controller,
            type_line: obj.card.type_line.to_string(),
            mana_cost: obj.card.mana_cost.to_string(),
            oracle_text: obj.card.oracle_text.clone(),
            power: creature.then(|| self.power(id)),
            toughness: creature.then(|| self.toughness(id)),
            tapped: obj.tapped,
            damage: obj.damage,
            counters: obj.plus_counters,
            token: obj.token,
            summoning_sick: obj.summoning_sick,
            attacking: self.attackers().contains(&id),
            blocking: blocks.iter().find(|(b, _)| *b == id).map(|(_, a)| *a),
        }
    }
}
