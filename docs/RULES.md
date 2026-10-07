# Implemented rules

This version uses Bulgarian bridge-belote, with the baseline described by
[Belot.BG](https://belot.bg/belot/rules/). Regional house rules differ. These
choices are explicit so friends can agree before playing:

- 32 cards, 7 through ace, teams in opposite seats, counterclockwise.
- Deal in packets of 3 and 2; auction; deal the remaining 3. Four opening passes
  produce a new deal by the next player. Three consecutive passes after a bid,
  double, or redouble end the auction.
- Ascending contracts: clubs, diamonds, hearts, spades, no trumps, all trumps.
  A higher contract cancels an earlier double/redouble. Only opponents of the
  declaring partnership may double, only that partnership may redouble.
- Always follow suit. When the led suit is trump, or in all trumps, raise if
  possible even if your partner is winning. If void, cut an opponent's winner
  with trump; overtrump if possible. If overtrumping is impossible, discard any
  card. If your partner is winning, a void player may discard freely. No trumps
  has no obligation to raise. All trumps has no dominant suit.
- Trump order: J, 9, A, 10, K, Q, 8, 7. Plain order: A, 10, K, Q, J, 9, 8, 7.
- Sequences use natural order 7, 8, 9, 10, J, Q, K, A: 3 = 20, 4 = 50,
  5 or more = 100. Longer sequences outrank shorter ones, then compare high
  cards. Equal strongest opposing sequences cancel both teams' sequence points;
  suits do not break ties. The winning partnership scores all its sequences.
- Four jacks = 200, four nines = 150, four aces/tens/kings/queens = 100. Fours
  compete separately from sequences and compare by trump rank. Four sevens or
  eights do not score. A card cannot belong to both a sequence and a four.
  Shorter sub-sequences are available when needed to avoid such an overlap.
  A single continuous run cannot be split into multiple scoring announcements.
- Declare when playing your first card; omitted declarations do not score.
  The UI defaults to the highest total compatible set. Bonuses are adjudicated
  when the first trick is collected. Declarations are not published before the
  owner plays that first card.
- Belote = 20 for K+Q in a trump suit. It is announced automatically when the
  first of the pair is played while leading, following, or cutting. Discarding
  off-suit in all trumps does not qualify. Belote is independent of the other
  declaration contests. No declarations or belote in no trumps.
- Last trick = 10, capot = an additional 90. No-trump card points including the
  last ten are doubled before adding the capot bonus.
- The bidding partnership must score strictly more raw points to make its
  contract. If defeated, its opponents receive the whole hand. Doubles and
  redoubles award the whole rounded hand total, multiplied by 2 or 4, to the
  raw-point winner; they remain active on capot. All bonuses are included.
- If tied, the defenders score their rounded-down share and the bidder's share
  carries forward. A doubled/redoubled tie carries the entire multiplied pot.
  Existing carryover accumulates through consecutive ties and is awarded once
  to the winner of the next decisive hand.
- Scores are recorded in tens. Suit contracts round up from 7, down through 5;
  if both remainders are 6, the lower-scoring team rounds up and the other down
  (106–56 becomes 10–6). All trumps rounds up from 5, down through 3;
  if both remainders are 4, the lower-scoring team rounds up (224–34 becomes
  22–4). A tied boundary rounds the hanging share up. No trumps uses ordinary
  rounding after doubling. The boundary examples follow the
  [Belot.pro rules](https://secure.belot.pro/ngames.php?gt=B&it=h).
- First team to at least 151 wins, provided it leads the other team, there are
  no unresolved hanging points, and the final hand is not capot. A capot always
  requires another decisive non-capot hand. No kirtik penalty or special
  announcement-based instant victories.

Practice bots bid a natural suit when no contract exists, otherwise pass, then
choose legal cards using their own hand and current public trick. They are for
trying the game or filling an empty chair, not expert competition.
