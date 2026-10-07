use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Create {
        name: String,
    },
    Join {
        name: String,
        code: String,
    },
    Resume {
        code: String,
        token: String,
    },
    Leave,
    Sit {
        seat: usize,
    },
    AddBot {
        seat: usize,
    },
    RemoveBot {
        seat: usize,
    },
    Start,
    Bid {
        bid: Bid,
    },
    Play {
        card: Card,
        declarations: Vec<usize>,
    },
    NextRound,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Session { code: String, token: String },
    State { room: Box<RoomView> },
    Error { message: String },
    Left,
    Pong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    pub name: String,
    pub connected: bool,
    pub bot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomView {
    pub code: String,
    pub seats: [Option<PlayerView>; 4],
    pub you: usize,
    pub host: usize,
    pub game: Option<GameView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameView {
    pub phase: Phase,
    pub round: u32,
    pub dealer: usize,
    pub turn: usize,
    pub contract: Option<ContractBid>,
    pub hand: Vec<Card>,
    pub hand_counts: [usize; 4],
    pub legal_cards: Vec<Card>,
    pub legal_bids: Vec<Bid>,
    pub bids: Vec<BidRecord>,
    pub trick: Vec<PlayedCard>,
    pub last_trick: Vec<PlayedCard>,
    pub last_winner: Option<usize>,
    pub tricks: [u8; 2],
    pub points: [u32; 2],
    pub scores: [u32; 2],
    pub hanging: u32,
    pub declarations: Vec<Declaration>,
    pub announcements: Vec<String>,
    pub history: Vec<RoundResult>,
    pub winner: Option<usize>,
}
