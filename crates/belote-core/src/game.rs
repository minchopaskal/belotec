use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Bidding,
    Playing,
    TrickEnd,
    RoundEnd,
    Redeal,
    MatchEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bid {
    Pass,
    Contract(Contract),
    Double,
    Redouble,
}
impl Bid {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "Pass",
            Self::Contract(c) => c.label(),
            Self::Double => "Double",
            Self::Redouble => "Redouble",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractBid {
    pub contract: Contract,
    pub bidder: usize,
    pub multiplier: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BidRecord {
    pub seat: usize,
    pub bid: Bid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclarationKind {
    Sequence { length: usize, high: Rank },
    Four { rank: Rank },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub cards: Vec<Card>,
    pub points: u32,
    pub label: String,
    /// Options from one continuous run cannot be split into multiple announcements.
    pub sequence_group: Option<Card>,
}
impl Declaration {
    pub fn conflicts_with(&self, other: &Self) -> bool {
        (self.sequence_group.is_some() && self.sequence_group == other.sequence_group)
            || self.cards.iter().any(|c| other.cards.contains(c))
    }
}

pub fn declarations(hand: &[Card]) -> Vec<Declaration> {
    let mut found = Vec::new();
    for rank in [
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ] {
        let cards: Vec<_> = hand.iter().copied().filter(|c| c.rank == rank).collect();
        if cards.len() == 4 {
            let points = match rank {
                Rank::Jack => 200,
                Rank::Nine => 150,
                _ => 100,
            };
            found.push(Declaration {
                kind: DeclarationKind::Four { rank },
                cards,
                points,
                label: format!("Four {}s · {points}", rank.label()),
                sequence_group: None,
            });
        }
    }
    for suit in Suit::ALL {
        let mut run = Vec::new();
        for rank in Rank::ALL.into_iter().map(Some).chain([None]) {
            if let Some(rank) = rank.filter(|r| hand.contains(&Card::new(suit, *r))) {
                run.push(Card::new(suit, rank));
            } else {
                for length in 3..=run.len() {
                    for cards in run.windows(length) {
                        let high = cards.last().unwrap().rank;
                        let points = if length == 3 {
                            20
                        } else if length == 4 {
                            50
                        } else {
                            100
                        };
                        found.push(Declaration {
                            kind: DeclarationKind::Sequence { length, high },
                            cards: cards.to_vec(),
                            points,
                            label: format!(
                                "{length} in {} to {} · {points}",
                                suit.symbol(),
                                high.label()
                            ),
                            sequence_group: run.first().copied(),
                        });
                    }
                }
                run.clear();
            }
        }
    }
    found
}

pub fn best_declarations(options: &[Declaration]) -> Vec<usize> {
    // At most eight cards can be used. Pruning overlaps avoids an exponential scan of long runs.
    fn visit(
        options: &[Declaration],
        start: usize,
        points: u32,
        chosen: &mut Vec<usize>,
        best: &mut (u32, (usize, u8), Vec<usize>),
    ) {
        let strongest = chosen
            .iter()
            .filter_map(|i| match options[*i].kind {
                DeclarationKind::Sequence { length, high } => Some((length, high as u8)),
                _ => None,
            })
            .max()
            .unwrap_or_default();
        if (points, strongest) > (best.0, best.1) {
            *best = (points, strongest, chosen.clone());
        }
        for i in start..options.len() {
            if chosen
                .iter()
                .all(|j| !options[i].conflicts_with(&options[*j]))
            {
                chosen.push(i);
                visit(options, i + 1, points + options[i].points, chosen, best);
                chosen.pop();
            }
        }
    }
    let mut best = (0, (0, 0), Vec::new());
    visit(options, 0, 0, &mut Vec::new(), &mut best);
    best.2
}

fn declaration_totals(chosen: &[Vec<Declaration>; 4]) -> [u32; 2] {
    let mut totals = [0; 2];
    // Sequences and four-of-a-kind compete independently in Bulgarian belote.
    for sequences in [true, false] {
        let mut best: [Option<(usize, u8)>; 2] = [None, None];
        let mut sums = [0; 2];
        for (seat, ds) in chosen.iter().enumerate() {
            for d in ds {
                let key = match d.kind {
                    DeclarationKind::Sequence { length, high } if sequences => {
                        Some((length, high as u8))
                    }
                    DeclarationKind::Four { rank } if !sequences => {
                        Some((d.points as usize, rank.strength(true)))
                    }
                    _ => None,
                };
                if let Some(key) = key {
                    best[seat % 2] = Some(best[seat % 2].map_or(key, |b| b.max(key)));
                    sums[seat % 2] += d.points;
                }
            }
        }
        if best[0] > best[1] {
            totals[0] += sums[0];
        }
        if best[1] > best[0] {
            totals[1] += sums[1];
        }
    }
    totals
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundResult {
    pub round: u32,
    pub contract: ContractBid,
    pub points: [u32; 2],
    pub earned: [u32; 2],
    pub scores: [u32; 2],
    pub capot: bool,
    pub outcome: String,
}

/// The full state stays on the server. Only `view(seat)` goes over the wire.
#[derive(Debug, Clone)]
pub struct Game {
    pub phase: Phase,
    pub round: u32,
    pub dealer: usize,
    pub turn: usize,
    pub contract: Option<ContractBid>,
    hands: [Vec<Card>; 4],
    remaining: [Vec<Card>; 4],
    passes: u8,
    bids: Vec<BidRecord>,
    trick: Vec<PlayedCard>,
    last_trick: Vec<PlayedCard>,
    last_winner: Option<usize>,
    tricks: [u8; 2],
    points: [u32; 2],
    scores: [u32; 2],
    hanging: u32,
    options: [Vec<Declaration>; 4],
    chosen: [Vec<Declaration>; 4],
    has_played: [bool; 4],
    announcements: Vec<String>,
    history: Vec<RoundResult>,
    winner: Option<usize>,
}

impl Game {
    pub fn new(deck: Vec<Card>) -> Result<Self, String> {
        let mut game = Self {
            phase: Phase::Bidding,
            round: 0,
            dealer: 3,
            turn: 0,
            contract: None,
            hands: Default::default(),
            remaining: Default::default(),
            passes: 0,
            bids: vec![],
            trick: vec![],
            last_trick: vec![],
            last_winner: None,
            tricks: [0; 2],
            points: [0; 2],
            scores: [0; 2],
            hanging: 0,
            options: Default::default(),
            chosen: Default::default(),
            has_played: [false; 4],
            announcements: vec![],
            history: vec![],
            winner: None,
        };
        game.deal(deck)?;
        Ok(game)
    }

    fn deal(&mut self, deck: Vec<Card>) -> Result<(), String> {
        if deck.len() != 32 || deck.iter().collect::<HashSet<_>>().len() != 32 {
            return Err("A deal needs 32 different cards.".into());
        }
        self.round += 1;
        self.phase = Phase::Bidding;
        self.turn = (self.dealer + 1) % 4;
        self.contract = None;
        self.passes = 0;
        self.hands = Default::default();
        self.remaining = Default::default();
        self.trick.clear();
        self.last_trick.clear();
        self.bids.clear();
        self.announcements.clear();
        self.last_winner = None;
        self.tricks = [0; 2];
        self.points = [0; 2];
        self.has_played = [false; 4];
        self.options = Default::default();
        self.chosen = Default::default();
        let mut cards = deck.into_iter();
        for (packet, count) in [3, 2, 3].into_iter().enumerate() {
            for offset in 1..=4 {
                let seat = (self.dealer + offset) % 4;
                let target = if packet == 2 {
                    &mut self.remaining[seat]
                } else {
                    &mut self.hands[seat]
                };
                target.extend(cards.by_ref().take(count));
            }
        }
        self.sort_hands();
        Ok(())
    }

    fn sort_hands(&mut self) {
        for hand in &mut self.hands {
            hand.sort_by_key(|c| {
                (
                    c.suit,
                    c.rank
                        .strength(self.contract.is_some_and(|b| b.contract.is_trump(c.suit))),
                )
            });
        }
    }

    pub fn next_round(&mut self, deck: Vec<Card>) -> Result<(), String> {
        if !matches!(self.phase, Phase::RoundEnd | Phase::Redeal) {
            return Err("Finish this hand first.".into());
        }
        // Validate before changing the dealer, so rejected input is atomic.
        if deck.len() != 32 || deck.iter().collect::<HashSet<_>>().len() != 32 {
            return Err("Invalid deck.".into());
        }
        self.dealer = (self.dealer + 1) % 4;
        self.deal(deck)
    }

    pub fn legal_bids(&self, seat: usize) -> Vec<Bid> {
        if seat >= 4 || self.phase != Phase::Bidding || self.turn != seat {
            return vec![];
        }
        let mut bids = vec![Bid::Pass];
        bids.extend(
            Contract::ALL
                .into_iter()
                .filter(|c| self.contract.is_none_or(|b| *c > b.contract))
                .map(Bid::Contract),
        );
        if let Some(b) = self.contract {
            if b.multiplier == 1 && seat % 2 != b.bidder % 2 {
                bids.push(Bid::Double);
            }
            if b.multiplier == 2 && seat % 2 == b.bidder % 2 {
                bids.push(Bid::Redouble);
            }
        }
        bids
    }

    pub fn bid(&mut self, seat: usize, bid: Bid) -> Result<(), String> {
        if !self.legal_bids(seat).contains(&bid) {
            return Err("That bid is not available on your turn.".into());
        }
        match bid {
            Bid::Pass => self.passes += 1,
            Bid::Contract(contract) => {
                self.contract = Some(ContractBid {
                    contract,
                    bidder: seat,
                    multiplier: 1,
                });
                self.passes = 0;
            }
            Bid::Double => {
                self.contract.as_mut().unwrap().multiplier = 2;
                self.passes = 0;
            }
            Bid::Redouble => {
                self.contract.as_mut().unwrap().multiplier = 4;
                self.passes = 0;
            }
        }
        self.bids.push(BidRecord { seat, bid });
        self.turn = (seat + 1) % 4;
        if self.contract.is_none() && self.passes == 4 {
            self.phase = Phase::Redeal;
        } else if self.contract.is_some() && self.passes == 3 {
            self.phase = Phase::Playing;
            self.turn = (self.dealer + 1) % 4;
            for s in 0..4 {
                self.hands[s].append(&mut self.remaining[s]);
                if self.contract.unwrap().contract != Contract::NoTrumps {
                    self.options[s] = declarations(&self.hands[s]);
                }
            }
            self.sort_hands();
        }
        Ok(())
    }

    pub fn legal_cards(&self, seat: usize) -> Vec<Card> {
        if seat >= 4 || self.phase != Phase::Playing || seat != self.turn {
            return vec![];
        }
        legal_cards(
            &self.hands[seat],
            &self.trick,
            self.contract.unwrap().contract,
            seat,
        )
    }

    pub fn play(&mut self, seat: usize, card: Card, selected: &[usize]) -> Result<(), String> {
        if !self.legal_cards(seat).contains(&card) {
            return Err(
                "Follow suit and raise or trump when required. Choose a highlighted card.".into(),
            );
        }
        if selected.len() > 8 {
            return Err("Too many declarations.".into());
        }
        let mut chosen: Vec<Declaration> = Vec::new();
        if self.has_played[seat] && !selected.is_empty() {
            return Err("Declare with your first card only.".into());
        }
        for i in selected {
            let d = self.options[seat].get(*i).ok_or("Invalid declaration.")?;
            if chosen.iter().any(|previous| d.conflicts_with(previous)) {
                return Err(
                    "Declarations cannot share cards or split one continuous run. Choose one."
                        .into(),
                );
            }
            chosen.push(d.clone());
        }
        for d in &chosen {
            self.announcements
                .push(format!("Seat {}: {}", seat + 1, d.label));
        }
        self.chosen[seat].extend(chosen);
        let contract = self.contract.unwrap().contract;
        let partner = match card.rank {
            Rank::King => Some(Rank::Queen),
            Rank::Queen => Some(Rank::King),
            _ => None,
        };
        let can_belote = self.trick.is_empty()
            || self.trick[0].card.suit == card.suit
            || contract.suit() == Some(card.suit);
        if contract.is_trump(card.suit)
            && can_belote
            && partner.is_some_and(|r| self.hands[seat].contains(&Card::new(card.suit, r)))
        {
            self.points[seat % 2] += 20;
            self.announcements.push(format!(
                "Seat {}: Belote {} · 20",
                seat + 1,
                card.suit.symbol()
            ));
        }
        self.hands[seat].retain(|c| *c != card);
        self.has_played[seat] = true;
        self.trick.push(PlayedCard { seat, card });
        self.turn = (seat + 1) % 4;
        if self.trick.len() == 4 {
            self.phase = Phase::TrickEnd;
        }
        Ok(())
    }

    /// Called after a short server-side viewing pause; clients cannot rush collection.
    pub fn collect_trick(&mut self) -> Result<(), String> {
        if self.phase != Phase::TrickEnd {
            return Err("The trick is not complete.".into());
        }
        let contract = self.contract.unwrap().contract;
        let winner = trick_winner(&self.trick, contract).unwrap().seat;
        let team = winner % 2;
        self.points[team] += self
            .trick
            .iter()
            .map(|p| p.card.points(contract))
            .sum::<u32>();
        self.tricks[team] += 1;
        if self.tricks.iter().sum::<u8>() == 1 {
            let totals = declaration_totals(&self.chosen);
            for (points, bonus) in self.points.iter_mut().zip(totals) {
                *points += bonus;
            }
            if totals != [0; 2] {
                self.announcements.push(format!(
                    "Declarations awarded: team 1 +{}, team 2 +{}",
                    totals[0], totals[1]
                ));
            }
        }
        self.last_trick = std::mem::take(&mut self.trick);
        self.last_winner = Some(winner);
        self.turn = winner;
        if self.hands.iter().all(Vec::is_empty) {
            self.points[team] += 10;
            if contract == Contract::NoTrumps {
                for p in &mut self.points {
                    *p *= 2;
                }
            }
            let capot = self.tricks[team] == 8;
            if capot {
                self.points[team] += 90;
            }
            self.finish_round(capot);
        } else {
            self.phase = Phase::Playing;
        }
        Ok(())
    }

    fn finish_round(&mut self, capot: bool) {
        let b = self.contract.unwrap();
        let taker = b.bidder % 2;
        let defender = 1 - taker;
        let mut earned = [0; 2];
        let total = (self.points[0] + self.points[1] + 5) / 10;
        let outcome;
        if self.points[0] == self.points[1] {
            outcome = "Tied hand · points carried forward".to_string();
            if b.multiplier > 1 {
                self.hanging += total * b.multiplier;
            } else {
                earned[defender] = self.points[defender] / 10;
                self.hanging += total - earned[defender];
            }
        } else {
            let winning = usize::from(self.points[1] > self.points[0]);
            if b.multiplier > 1 || winning != taker {
                earned[winning] = total * b.multiplier;
            } else {
                earned = self
                    .points
                    .map(|p| (p + if b.contract.suit().is_some() { 3 } else { 5 }) / 10);
                if b.contract.suit().is_some() && self.points.iter().all(|p| p % 10 == 6) {
                    earned[1 - winning] += 1;
                }
                if b.contract == Contract::AllTrumps && self.points.iter().all(|p| p % 10 == 4) {
                    earned[1 - winning] += 1;
                }
            }
            earned[winning] += self.hanging;
            self.hanging = 0;
            outcome = if winning == taker {
                "Contract made"
            } else {
                "Contract defeated"
            }
            .into();
        }
        for (score, add) in self.scores.iter_mut().zip(earned) {
            *score += add;
        }
        self.history.push(RoundResult {
            round: self.round,
            contract: b,
            points: self.points,
            earned,
            scores: self.scores,
            capot,
            outcome,
        });
        self.phase = Phase::RoundEnd;
        if !capot
            && self.hanging == 0
            && self.scores[0] != self.scores[1]
            && self.scores.iter().any(|s| *s >= 151)
        {
            self.winner = Some(usize::from(self.scores[1] > self.scores[0]));
            self.phase = Phase::MatchEnd;
        }
    }

    pub fn view(&self, seat: usize) -> GameView {
        assert!(seat < 4, "views require an authenticated seat");
        GameView {
            phase: self.phase,
            round: self.round,
            dealer: self.dealer,
            turn: self.turn,
            contract: self.contract,
            hand: self.hands[seat].clone(),
            hand_counts: self.hands.each_ref().map(Vec::len),
            legal_cards: self.legal_cards(seat),
            legal_bids: self.legal_bids(seat),
            bids: self.bids.clone(),
            trick: self.trick.clone(),
            last_trick: self.last_trick.clone(),
            last_winner: self.last_winner,
            tricks: self.tricks,
            points: self.points,
            scores: self.scores,
            hanging: self.hanging,
            declarations: if self.phase == Phase::Playing && !self.has_played[seat] {
                self.options[seat].clone()
            } else {
                vec![]
            },
            announcements: self.announcements.clone(),
            history: self.history.clone(),
            winner: self.winner,
        }
    }

    pub fn bot_bid(&self) -> Bid {
        let hand = &self.hands[self.turn];
        // Deliberately modest bots: one natural suit bid, otherwise pass.
        if self.contract.is_some() {
            return Bid::Pass;
        }
        let best = Contract::ALL
            .into_iter()
            .filter(|c| c.suit().is_some())
            .max_by_key(|c| {
                hand.iter()
                    .filter(|card| c.is_trump(card.suit))
                    .map(|card| card.points(*c) + 8)
                    .sum::<u32>()
            })
            .unwrap();
        Bid::Contract(best)
    }

    pub fn bot_play(&self) -> (Card, Vec<usize>) {
        let seat = self.turn;
        let contract = self.contract.unwrap().contract;
        let legal = self.legal_cards(seat);
        let winner = trick_winner(&self.trick, contract);
        let partner_winning = winner.is_some_and(|p| p.seat % 2 == seat % 2);
        let card = if partner_winning {
            legal.iter().max_by_key(|c| c.points(contract)).copied()
        } else {
            legal
                .iter()
                .filter(|c| winner.is_none_or(|p| beats(**c, p.card, contract)))
                .min_by_key(|c| c.points(contract))
                .copied()
                .or_else(|| legal.iter().min_by_key(|c| c.points(contract)).copied())
        }
        .unwrap();
        (
            card,
            if self.has_played[seat] {
                vec![]
            } else {
                best_declarations(&self.options[seat])
            },
        )
    }
}

#[cfg(test)]
mod tests;
