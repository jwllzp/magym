//! What players decide: the decisions the engine asks for, and the actions
//! that answer them.

use serde::{Deserialize, Serialize};

use super::mana::Mana;
use super::state::{ObjectId, PlayerId, Target};

/// A decision the engine is waiting for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub player: PlayerId,
    pub kind: DecisionKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum DecisionKind {
    /// Choose who takes the first turn (CR 103.1). Answer with
    /// [`Action::ChooseStartingPlayer`].
    ChooseStartingPlayer,
    /// Keep the hand or take a mulligan (CR 103.5). Answer with
    /// [`Action::Keep`] or [`Action::Mulligan`].
    Mulligan { hand_size: usize, mulligans: u32 },
    /// Put `count` cards from hand on the bottom of the library after a
    /// mulligan (CR 103.5). Answer with [`Action::PutOnBottom`].
    PutOnBottom { count: usize },
    /// The player has priority (CR 117). Answer with [`Action::Pass`] or any
    /// action from [`Game::legal_actions`](super::Game::legal_actions).
    Priority,
    /// Declare attackers (CR 508.1) from `candidates`. Answer with
    /// [`Action::DeclareAttackers`].
    DeclareAttackers { candidates: Vec<ObjectId> },
    /// Declare blockers (CR 509.1): each candidate blocker with the
    /// attackers it may block. Answer with [`Action::DeclareBlockers`].
    DeclareBlockers { candidates: Vec<BlockOption> },
    /// Divide an attacking creature's combat damage among the creatures
    /// blocking it and, with trample, the defending player (CR 510.1c,
    /// 702.19b). Answer with [`Action::AssignCombatDamage`].
    AssignCombatDamage {
        attacker: ObjectId,
        power: u32,
        /// Each blocker with the damage that would be lethal to it.
        blockers: Vec<(ObjectId, u32)>,
        /// The player excess damage can go to, if the attacker has trample.
        trample_to: Option<PlayerId>,
    },
    /// Discard `count` cards (CR 514.1, 701.9). Answer with
    /// [`Action::Discard`].
    Discard { count: usize },
    /// Legend rule (CR 704.5j): choose which of these to keep. Answer with
    /// [`Action::KeepLegend`].
    LegendRule { choices: Vec<ObjectId> },
    /// Order triggered abilities (CR 603.3b). The first one listed is put on
    /// the stack first and resolves last. Answer with
    /// [`Action::OrderTriggers`].
    OrderTriggers { triggers: Vec<TriggerInfo> },
    /// Choose the mode and targets of a triggered ability as it's put on the
    /// stack (CR 603.3c, 603.3d). Answer with [`Action::ChooseTargets`].
    ChooseTargets {
        source: ObjectId,
        options: Vec<TargetOptions>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockOption {
    pub blocker: ObjectId,
    pub attackers: Vec<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerInfo {
    pub source: ObjectId,
    pub name: String,
    pub ability: usize,
}

/// For one mode (or `None` for a non-modal ability), the legal choices for
/// each target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetOptions {
    pub mode: Option<usize>,
    pub targets: Vec<Vec<Target>>,
}

/// Activating a mana ability of `source` that produces `mana` (CR 605).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaActivation {
    pub source: ObjectId,
    pub mana: Vec<Mana>,
}

/// An answer to a decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    ChooseStartingPlayer {
        player: PlayerId,
    },
    Keep,
    Mulligan,
    PutOnBottom {
        cards: Vec<ObjectId>,
    },
    /// Pass priority (CR 117.3d).
    Pass,
    /// Play a land (CR 305.1, 505.6b).
    PlayLand {
        card: ObjectId,
    },
    /// Cast a spell from hand (CR 601.2). `mana` lists the mana abilities to
    /// activate while paying (CR 601.2g); mana already in the pool is used
    /// too.
    CastSpell {
        card: ObjectId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<usize>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        targets: Vec<Target>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mana: Vec<ManaActivation>,
    },
    /// Activate a non-mana activated ability (CR 602.2); `ability` indexes
    /// the source's scripted activated abilities.
    ActivateAbility {
        source: ObjectId,
        ability: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<usize>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        targets: Vec<Target>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mana: Vec<ManaActivation>,
    },
    /// Activate a mana ability while holding priority (CR 117.1d).
    ActivateManaAbility(ManaActivation),
    DeclareAttackers {
        attackers: Vec<ObjectId>,
    },
    /// Pairs of (blocker, attacker).
    DeclareBlockers {
        blocks: Vec<(ObjectId, ObjectId)>,
    },
    AssignCombatDamage {
        assignments: Vec<(Target, u32)>,
    },
    Discard {
        cards: Vec<ObjectId>,
    },
    KeepLegend {
        card: ObjectId,
    },
    /// A permutation of the listed triggers' indices.
    OrderTriggers {
        order: Vec<usize>,
    },
    ChooseTargets {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<usize>,
        targets: Vec<Target>,
    },
    /// Concede the game (CR 104.3a). Allowed at any time.
    Concede,
}
