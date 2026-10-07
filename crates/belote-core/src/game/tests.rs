use super::*;
use rand::{seq::SliceRandom, SeedableRng};

fn c(s: Suit, r: Rank) -> Card {
    Card::new(s, r)
}
fn p(seat: usize, card: Card) -> PlayedCard {
    PlayedCard { seat, card }
}
fn playing(contract: Contract) -> Game {
    let mut g = Game::new(deck()).unwrap();
    g.bid(0, Bid::Contract(contract)).unwrap();
    for seat in [1, 2, 3] {
        g.bid(seat, Bid::Pass).unwrap();
    }
    g
}

#[test]
fn deck_values_and_rankings() {
    let d = deck();
    assert_eq!(d.len(), 32);
    assert_eq!(d.iter().collect::<HashSet<_>>().len(), 32);
    assert_eq!(
        d.iter().map(|c| c.points(Contract::Spades)).sum::<u32>(),
        152
    );
    assert_eq!(
        d.iter().map(|c| c.points(Contract::AllTrumps)).sum::<u32>(),
        248
    );
    assert_eq!(
        d.iter().map(|c| c.points(Contract::NoTrumps)).sum::<u32>(),
        120
    );
    assert!(beats(
        c(Suit::Spades, Rank::Jack),
        c(Suit::Spades, Rank::Ace),
        Contract::Spades
    ));
    assert!(!beats(
        c(Suit::Hearts, Rank::Jack),
        c(Suit::Spades, Rank::Seven),
        Contract::AllTrumps
    ));
    assert!(beats(
        c(Suit::Spades, Rank::Seven),
        c(Suit::Hearts, Rank::Ace),
        Contract::Spades
    ));
}

#[test]
fn invalid_decks_are_rejected() {
    let mut d = deck();
    d[1] = d[0];
    assert!(Game::new(d).is_err());
    assert!(Game::new(vec![]).is_err());
}

#[test]
fn five_cards_then_three_and_private_views() {
    let mut g = Game::new(deck()).unwrap();
    assert_eq!(g.view(0).hand_counts, [5; 4]);
    assert_eq!(g.view(0).hand.len(), 5);
    assert!(g.bid(1, Bid::Pass).is_err());
    g.bid(0, Bid::Contract(Contract::Hearts)).unwrap();
    for s in [1, 2, 3] {
        g.bid(s, Bid::Pass).unwrap();
    }
    assert_eq!(g.phase, Phase::Playing);
    assert_eq!(g.view(0).hand_counts, [8; 4]);
    let hands = (0..4).flat_map(|s| g.view(s).hand).collect::<Vec<_>>();
    assert_eq!(hands.iter().collect::<HashSet<_>>().len(), 32);
    let json = serde_json::to_value(g.view(0)).unwrap();
    assert!(json.get("hands").is_none());
    assert!(json.get("remaining").is_none());
}

#[test]
fn four_passes_rotate_dealer_without_scoring() {
    let mut g = Game::new(deck()).unwrap();
    for s in 0..4 {
        g.bid(s, Bid::Pass).unwrap();
    }
    assert_eq!(g.phase, Phase::Redeal);
    assert!(g.history.is_empty());
    g.next_round(deck()).unwrap();
    assert_eq!(g.dealer, 0);
    assert_eq!(g.turn, 1);
    assert_eq!(g.scores, [0, 0]);
}

#[test]
fn doubles_are_team_restricted_and_higher_bids_reset_them() {
    let mut g = Game::new(deck()).unwrap();
    g.bid(0, Bid::Contract(Contract::Clubs)).unwrap();
    assert!(g.legal_bids(1).contains(&Bid::Double));
    g.bid(1, Bid::Double).unwrap();
    assert!(g.legal_bids(2).contains(&Bid::Redouble));
    g.bid(2, Bid::Redouble).unwrap();
    assert_eq!(g.contract.unwrap().multiplier, 4);
    g.bid(3, Bid::Contract(Contract::NoTrumps)).unwrap();
    assert_eq!(g.contract.unwrap().multiplier, 1);
    assert!(!g.legal_bids(0).contains(&Bid::Contract(Contract::Hearts)));
}

#[test]
fn partner_cannot_double_own_bid() {
    let mut g = Game::new(deck()).unwrap();
    g.bid(0, Bid::Contract(Contract::AllTrumps)).unwrap();
    g.bid(1, Bid::Pass).unwrap();
    assert!(!g.legal_bids(2).contains(&Bid::Double));
    assert!(g.bid(2, Bid::Double).is_err());
}

#[test]
fn follow_suit_before_cutting() {
    let hand = vec![c(Suit::Hearts, Rank::Seven), c(Suit::Spades, Rank::Jack)];
    assert_eq!(
        legal_cards(
            &hand,
            &[p(0, c(Suit::Hearts, Rank::Ace))],
            Contract::Spades,
            1
        ),
        vec![hand[0]]
    );
}

#[test]
fn raise_trump_even_when_partner_is_winning() {
    let hand = vec![c(Suit::Spades, Rank::Seven), c(Suit::Spades, Rank::Jack)];
    assert_eq!(
        legal_cards(
            &hand,
            &[p(0, c(Suit::Spades, Rank::Ace))],
            Contract::Spades,
            2
        ),
        vec![hand[1]]
    );
}

#[test]
fn all_trumps_raise_only_in_led_suit() {
    let hand = vec![
        c(Suit::Hearts, Rank::Seven),
        c(Suit::Hearts, Rank::Nine),
        c(Suit::Spades, Rank::Jack),
    ];
    assert_eq!(
        legal_cards(
            &hand,
            &[p(0, c(Suit::Hearts, Rank::Ace))],
            Contract::AllTrumps,
            2
        ),
        vec![hand[1]]
    );
    assert_eq!(
        legal_cards(
            &hand,
            &[p(0, c(Suit::Clubs, Rank::Seven))],
            Contract::AllTrumps,
            2
        ),
        hand
    );
}

#[test]
fn cut_opponents_but_can_discard_on_partner() {
    let hand = vec![c(Suit::Diamonds, Rank::Seven), c(Suit::Spades, Rank::Eight)];
    let trick = vec![p(0, c(Suit::Hearts, Rank::Ace))];
    assert_eq!(
        legal_cards(&hand, &trick, Contract::Spades, 1),
        vec![hand[1]]
    );
    assert_eq!(legal_cards(&hand, &trick, Contract::Spades, 2), hand);
}

#[test]
fn overtrump_required_but_free_discard_if_unable() {
    let trick = vec![
        p(0, c(Suit::Hearts, Rank::Ace)),
        p(1, c(Suit::Spades, Rank::Nine)),
    ];
    let low = vec![c(Suit::Diamonds, Rank::Seven), c(Suit::Spades, Rank::Eight)];
    assert_eq!(legal_cards(&low, &trick, Contract::Spades, 2), low);
    let high = vec![low[0], c(Suit::Spades, Rank::Jack)];
    assert_eq!(
        legal_cards(&high, &trick, Contract::Spades, 2),
        vec![high[1]]
    );
}

#[test]
fn no_trumps_never_requires_raising() {
    let hand = vec![c(Suit::Hearts, Rank::Seven), c(Suit::Hearts, Rank::Ace)];
    assert_eq!(
        legal_cards(
            &hand,
            &[p(0, c(Suit::Hearts, Rank::King))],
            Contract::NoTrumps,
            1
        ),
        hand
    );
    assert!(playing(Contract::NoTrumps)
        .options
        .iter()
        .all(Vec::is_empty));
}

#[test]
fn invalid_moves_and_declarations_do_not_mutate_state() {
    let mut g = playing(Contract::Spades);
    let before = g.view(0);
    assert!(g.play(1, g.hands[1][0], &[]).is_err());
    assert_eq!(g.view(0), before);
    assert!(g.play(0, g.hands[0][0], &[999]).is_err());
    assert_eq!(g.view(0), before);
}

#[test]
fn overlapping_four_and_run_can_keep_shorter_nonoverlapping_run() {
    let hand = vec![
        c(Suit::Hearts, Rank::Seven),
        c(Suit::Hearts, Rank::Eight),
        c(Suit::Hearts, Rank::Nine),
        c(Suit::Hearts, Rank::Ten),
        c(Suit::Spades, Rank::Ten),
        c(Suit::Clubs, Rank::Ten),
        c(Suit::Diamonds, Rank::Ten),
        c(Suit::Clubs, Rank::Ace),
    ];
    let ds = declarations(&hand);
    let best = best_declarations(&ds);
    assert_eq!(best.iter().map(|i| ds[*i].points).sum::<u32>(), 120);
    let mut g = playing(Contract::Hearts);
    g.hands[0] = hand;
    g.options[0] = ds;
    let four = g.options[0]
        .iter()
        .position(|d| matches!(d.kind, DeclarationKind::Four { .. }))
        .unwrap();
    let run = g.options[0]
        .iter()
        .position(|d| matches!(d.kind, DeclarationKind::Sequence { length: 4, .. }))
        .unwrap();
    let before = g.view(0);
    assert!(g.play(0, g.hands[0][0], &[four, run]).is_err());
    assert_eq!(g.view(0), before);
}

#[test]
fn tied_sequences_cancel_and_fours_compete_separately() {
    let seq = |suit| {
        declarations(&[
            c(suit, Rank::Jack),
            c(suit, Rank::Queen),
            c(suit, Rank::King),
        ])
    };
    let mut ds: [Vec<Declaration>; 4] = Default::default();
    ds[0] = seq(Suit::Hearts);
    ds[1] = seq(Suit::Spades);
    assert_eq!(declaration_totals(&ds), [0, 0]);
    ds[0].extend(declarations(&Suit::ALL.map(|s| c(s, Rank::Jack))));
    assert_eq!(declaration_totals(&ds), [200, 0]);
    ds[1].clear();
    ds[1].extend(declarations(&Suit::ALL.map(|s| c(s, Rank::Ace))));
    assert_eq!(declaration_totals(&ds), [220, 0]);
}

#[test]
fn belote_is_awarded_once_and_not_on_offsuit_all_trump_discard() {
    let mut g = playing(Contract::AllTrumps);
    g.hands[0] = vec![c(Suit::Hearts, Rank::Queen), c(Suit::Hearts, Rank::King)];
    g.play(0, c(Suit::Hearts, Rank::Queen), &[]).unwrap();
    assert_eq!(g.points, [20, 0]);
    g.turn = 0;
    g.trick.clear();
    g.play(0, c(Suit::Hearts, Rank::King), &[]).unwrap();
    assert_eq!(g.points, [20, 0]);
    let mut g = playing(Contract::AllTrumps);
    g.turn = 1;
    g.trick = vec![p(0, c(Suit::Clubs, Rank::Jack))];
    g.hands[1] = vec![c(Suit::Hearts, Rank::Queen), c(Suit::Hearts, Rank::King)];
    g.play(1, c(Suit::Hearts, Rank::Queen), &[]).unwrap();
    assert_eq!(g.points, [0, 0]);
}

#[test]
fn score_made_failed_double_and_redouble() {
    for (multiplier, points, expected) in [
        (1, [100, 62], [10, 6]),
        (1, [62, 100], [0, 16]),
        (2, [100, 62], [32, 0]),
        (4, [62, 100], [0, 64]),
    ] {
        let mut g = playing(Contract::Spades);
        g.contract.as_mut().unwrap().multiplier = multiplier;
        g.points = points;
        g.finish_round(false);
        assert_eq!(g.scores, expected);
    }
}

#[test]
fn rounding_handles_both_bulgarian_boundary_cases() {
    for (contract, points, expected) in [
        (Contract::Spades, [106, 56], [10, 6]),
        (Contract::Spades, [87, 75], [9, 7]),
        (Contract::Spades, [85, 77], [8, 8]),
        (Contract::AllTrumps, [224, 34], [22, 4]),
        (Contract::AllTrumps, [194, 164], [19, 17]),
    ] {
        let mut g = playing(contract);
        g.points = points;
        g.finish_round(false);
        assert_eq!(g.scores, expected);
    }
}

#[test]
fn tied_pot_accumulates_and_is_awarded_once() {
    let mut g = playing(Contract::Spades);
    g.points = [106, 106];
    g.finish_round(false);
    assert_eq!(g.scores, [0, 10]);
    assert_eq!(g.hanging, 11);
    g.points = [81, 81];
    g.contract.as_mut().unwrap().multiplier = 2;
    g.finish_round(false);
    assert_eq!(g.scores, [0, 10]);
    assert_eq!(g.hanging, 43);
    g.points = [100, 62];
    g.contract.as_mut().unwrap().multiplier = 1;
    g.finish_round(false);
    assert_eq!(g.scores, [53, 16]);
    assert_eq!(g.hanging, 0);
}

#[test]
fn no_trump_capot_doubles_cards_but_not_the_90_bonus() {
    let mut g = playing(Contract::NoTrumps);
    g.points = [100, 0];
    g.tricks = [7, 0];
    g.hands = Default::default();
    g.trick = vec![
        p(0, c(Suit::Clubs, Rank::Ace)),
        p(1, c(Suit::Clubs, Rank::King)),
        p(2, c(Suit::Clubs, Rank::Queen)),
        p(3, c(Suit::Clubs, Rank::Jack)),
    ];
    g.phase = Phase::TrickEnd;
    g.collect_trick().unwrap();
    assert_eq!(g.points, [350, 0]);
    assert_eq!(g.scores, [35, 0]);
}

#[test]
fn first_to_151_but_cannot_go_out_on_capot_or_a_tie() {
    let mut g = playing(Contract::Spades);
    g.scores = [140, 140];
    g.points = [252, 0];
    g.finish_round(true);
    assert_eq!(g.phase, Phase::RoundEnd);
    g.points = [100, 62];
    g.finish_round(false);
    assert_eq!(g.phase, Phase::MatchEnd);
    assert_eq!(g.winner, Some(0));
    let mut g = playing(Contract::Spades);
    g.scores = [150, 152];
    g.points = [100, 62];
    g.finish_round(false);
    assert_eq!(g.phase, Phase::MatchEnd);
    assert_eq!(g.winner, Some(0));
    let mut g = playing(Contract::Spades);
    g.scores = [150, 154];
    g.points = [100, 62];
    g.finish_round(false);
    assert_eq!(g.scores, [160, 160]);
    assert_eq!(g.phase, Phase::RoundEnd);
}

#[test]
fn seeded_complete_hands_conserve_cards_for_every_contract() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for contract in Contract::ALL {
        for _ in 0..80 {
            let mut d = deck();
            d.shuffle(&mut rng);
            let mut g = Game::new(d).unwrap();
            g.bid(0, Bid::Contract(contract)).unwrap();
            for s in [1, 2, 3] {
                g.bid(s, Bid::Pass).unwrap();
            }
            let mut played = HashSet::new();
            for _ in 0..8 {
                for _ in 0..4 {
                    let seat = g.turn;
                    let (card, ds) = g.bot_play();
                    assert!(played.insert(card));
                    g.play(seat, card, &ds).unwrap();
                }
                assert_eq!(g.phase, Phase::TrickEnd);
                g.collect_trick().unwrap();
            }
            assert_eq!(played.len(), 32);
            assert_eq!(g.tricks.iter().sum::<u8>(), 8);
            assert!(g.hands.iter().all(Vec::is_empty));
            assert_eq!(g.phase, Phase::RoundEnd);
            let base = match contract {
                Contract::AllTrumps => 258,
                Contract::NoTrumps => 260,
                _ => 162,
            };
            assert!(g.points.iter().sum::<u32>() >= base);
            assert_eq!(g.history.len(), 1);
        }
    }
}

#[test]
fn bots_finish_a_complete_match() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(913);
    let mut g = Game::new(deck()).unwrap();
    for _ in 0..10000 {
        match g.phase {
            Phase::Bidding => g.bid(g.turn, g.bot_bid()).unwrap(),
            Phase::Playing => {
                let (c, ds) = g.bot_play();
                g.play(g.turn, c, &ds).unwrap();
            }
            Phase::TrickEnd => g.collect_trick().unwrap(),
            Phase::RoundEnd | Phase::Redeal => {
                let mut d = deck();
                d.shuffle(&mut rng);
                g.next_round(d).unwrap();
            }
            Phase::MatchEnd => {
                assert!(g.scores[g.winner.unwrap()] >= 151);
                return;
            }
        }
    }
    panic!("match did not finish");
}

#[test]
fn long_runs_are_not_counted_twice_and_defaults_prefer_the_strongest_run() {
    let hand = Rank::ALL.map(|r| c(Suit::Clubs, r));
    let options = declarations(&hand);
    let selected = best_declarations(&options);
    assert_eq!(selected.len(), 1);
    assert_eq!(options[selected[0]].points, 100);
    assert!(matches!(
        options[selected[0]].kind,
        DeclarationKind::Sequence { length: 8, .. }
    ));
    let low = options.iter().find(|d| d.cards == hand[..3]).unwrap();
    let high = options.iter().find(|d| d.cards == hand[3..]).unwrap();
    assert!(low.conflicts_with(high));
    // Two genuinely separate runs in the same suit are still independent.
    let separate = [
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ]
    .map(|r| c(Suit::Clubs, r));
    let options = declarations(&separate);
    assert_eq!(best_declarations(&options).len(), 2);
}
