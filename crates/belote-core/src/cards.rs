use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}
impl Suit {
    pub const ALL: [Self; 4] = [Self::Clubs, Self::Diamonds, Self::Hearts, Self::Spades];
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Clubs => "♣",
            Self::Diamonds => "♦",
            Self::Hearts => "♥",
            Self::Spades => "♠",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Clubs => "Clubs",
            Self::Diamonds => "Diamonds",
            Self::Hearts => "Hearts",
            Self::Spades => "Spades",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Rank {
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}
impl Rank {
    pub const ALL: [Self; 8] = [
        Self::Seven,
        Self::Eight,
        Self::Nine,
        Self::Ten,
        Self::Jack,
        Self::Queen,
        Self::King,
        Self::Ace,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
            Self::Ace => "A",
        }
    }
    pub fn strength(self, trump: bool) -> u8 {
        match (trump, self) {
            (_, Self::Seven) => 0,
            (_, Self::Eight) => 1,
            (true, Self::Queen) => 2,
            (true, Self::King) => 3,
            (true, Self::Ten) => 4,
            (true, Self::Ace) => 5,
            (true, Self::Nine) => 6,
            (true, Self::Jack) => 7,
            (false, Self::Nine) => 2,
            (false, Self::Jack) => 3,
            (false, Self::Queen) => 4,
            (false, Self::King) => 5,
            (false, Self::Ten) => 6,
            (false, Self::Ace) => 7,
        }
    }
    pub fn points(self, trump: bool) -> u32 {
        match self {
            Self::Jack if trump => 20,
            Self::Nine if trump => 14,
            Self::Ace => 11,
            Self::Ten => 10,
            Self::King => 4,
            Self::Queen => 3,
            Self::Jack => 2,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}
impl Card {
    pub fn new(suit: Suit, rank: Rank) -> Self {
        Self { suit, rank }
    }
    pub fn asset(self) -> String {
        format!(
            "/assets/cards/card{}{}.png",
            self.suit.name(),
            self.rank.label()
        )
    }
    pub fn points(self, contract: Contract) -> u32 {
        self.rank.points(contract.is_trump(self.suit))
    }
}
impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank.label(), self.suit.symbol())
    }
}
pub fn deck() -> Vec<Card> {
    Suit::ALL
        .into_iter()
        .flat_map(|suit| Rank::ALL.map(|rank| Card { suit, rank }))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Contract {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
    NoTrumps,
    AllTrumps,
}
impl Contract {
    pub const ALL: [Self; 6] = [
        Self::Clubs,
        Self::Diamonds,
        Self::Hearts,
        Self::Spades,
        Self::NoTrumps,
        Self::AllTrumps,
    ];
    pub fn suit(self) -> Option<Suit> {
        match self {
            Self::Clubs => Some(Suit::Clubs),
            Self::Diamonds => Some(Suit::Diamonds),
            Self::Hearts => Some(Suit::Hearts),
            Self::Spades => Some(Suit::Spades),
            _ => None,
        }
    }
    pub fn is_trump(self, suit: Suit) -> bool {
        self == Self::AllTrumps || self.suit() == Some(suit)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Clubs => "Clubs",
            Self::Diamonds => "Diamonds",
            Self::Hearts => "Hearts",
            Self::Spades => "Spades",
            Self::NoTrumps => "No trumps",
            Self::AllTrumps => "All trumps",
        }
    }
    pub fn symbol(self) -> &'static str {
        self.suit()
            .map(Suit::symbol)
            .unwrap_or(if self == Self::AllTrumps { "AT" } else { "NT" })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayedCard {
    pub seat: usize,
    pub card: Card,
}

pub fn beats(card: Card, current: Card, contract: Contract) -> bool {
    if card.suit == current.suit {
        card.rank.strength(contract.is_trump(card.suit))
            > current.rank.strength(contract.is_trump(current.suit))
    } else {
        contract.suit() == Some(card.suit)
    }
}

pub fn trick_winner(trick: &[PlayedCard], contract: Contract) -> Option<PlayedCard> {
    trick.iter().copied().reduce(|winner, next| {
        if beats(next.card, winner.card, contract) {
            next
        } else {
            winner
        }
    })
}

/// Bulgarian obligations: follow; raise in trump/AT; cut opponents, unless unable to overtrump.
pub fn legal_cards(
    hand: &[Card],
    trick: &[PlayedCard],
    contract: Contract,
    seat: usize,
) -> Vec<Card> {
    let Some(first) = trick.first() else {
        return hand.to_vec();
    };
    let following: Vec<_> = hand
        .iter()
        .copied()
        .filter(|c| c.suit == first.card.suit)
        .collect();
    if !following.is_empty() {
        if contract.is_trump(first.card.suit) {
            let high = trick
                .iter()
                .filter(|p| p.card.suit == first.card.suit)
                .map(|p| p.card.rank.strength(true))
                .max()
                .unwrap();
            let higher: Vec<_> = following
                .iter()
                .copied()
                .filter(|c| c.rank.strength(true) > high)
                .collect();
            if !higher.is_empty() {
                return higher;
            }
        }
        return following;
    }
    if let Some(trump) = contract.suit() {
        let winner = trick_winner(trick, contract).unwrap();
        if winner.seat % 2 != seat % 2 {
            let cuts: Vec<_> = hand
                .iter()
                .copied()
                .filter(|c| c.suit == trump && beats(*c, winner.card, contract))
                .collect();
            if !cuts.is_empty() {
                return cuts;
            }
        }
    }
    hand.to_vec()
}
