use belote_core::*;
use gloo_timers::callback::{Interval, Timeout};
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{Event, HtmlInputElement, MessageEvent, WebSocket};
use yew::prelude::*;

fn window() -> web_sys::Window {
    web_sys::window().expect("browser window")
}
fn stored(key: &str, session: bool) -> Option<String> {
    let storage = if session {
        window().session_storage()
    } else {
        window().local_storage()
    };
    storage
        .ok()
        .flatten()
        .and_then(|s| s.get_item(key).ok().flatten())
}
fn save(key: &str, value: &str, session: bool) {
    let storage = if session {
        window().session_storage()
    } else {
        window().local_storage()
    };
    if let Ok(Some(s)) = storage {
        let _ = s.set_item(key, value);
    }
}
fn clear_session() {
    save("belote.session", "", true);
}

struct Socket {
    ws: WebSocket,
    _open: Closure<dyn FnMut(Event)>,
    _close: Closure<dyn FnMut(Event)>,
    _message: Closure<dyn FnMut(MessageEvent)>,
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.ws.set_onopen(None);
        self.ws.set_onclose(None);
        self.ws.set_onerror(None);
        self.ws.set_onmessage(None);
        let _ = self.ws.close();
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Menu,
    Lobby,
}
#[derive(Clone, Copy, PartialEq)]
enum Modal {
    Settings,
    Rules,
    Scores,
    Leave,
}
enum Msg {
    Connect,
    Opened(u32),
    Closed(u32),
    Received(u32, ServerMessage),
    Page(Page),
    Modal(Option<Modal>),
    Name(String),
    Code(String),
    Create,
    Join,
    Send(ClientMessage),
    Play(Card),
    SelectDeclaration(usize),
    Sound,
    CopyInvite,
    Notice(String),
    Dismiss,
    Heartbeat,
}

struct App {
    socket: Option<Socket>,
    epoch: u32,
    connected: bool,
    retry: Option<Timeout>,
    retries: u32,
    _heartbeat: Interval,
    page: Page,
    modal: Option<Modal>,
    name: String,
    code: String,
    room: Option<RoomView>,
    error: Option<String>,
    notice: Option<String>,
    busy: bool,
    selected: Vec<usize>,
    sound: bool,
    audio: Option<web_sys::AudioContext>,
}

impl App {
    fn connect(&mut self, ctx: &Context<Self>) {
        self.retry = None;
        self.socket = None;
        self.epoch += 1;
        let epoch = self.epoch;
        let location = window().location();
        let scheme = if location.protocol().unwrap_or_default() == "https:" {
            "wss"
        } else {
            "ws"
        };
        let url = format!("{scheme}://{}/ws", location.host().unwrap_or_default());
        match WebSocket::new(&url) {
            Ok(ws) => {
                let link = ctx.link().clone();
                let open =
                    Closure::wrap(
                        Box::new(move |_: Event| link.send_message(Msg::Opened(epoch)))
                            as Box<dyn FnMut(Event)>,
                    );
                let link = ctx.link().clone();
                let close =
                    Closure::wrap(
                        Box::new(move |_: Event| link.send_message(Msg::Closed(epoch)))
                            as Box<dyn FnMut(Event)>,
                    );
                let link = ctx.link().clone();
                let message = Closure::wrap(Box::new(move |e: MessageEvent| {
                    if let Some(text) = e.data().as_string() {
                        if let Ok(message) = serde_json::from_str(&text) {
                            link.send_message(Msg::Received(epoch, message));
                        }
                    }
                }) as Box<dyn FnMut(MessageEvent)>);
                ws.set_onopen(Some(open.as_ref().unchecked_ref()));
                ws.set_onclose(Some(close.as_ref().unchecked_ref()));
                ws.set_onerror(Some(close.as_ref().unchecked_ref()));
                ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
                self.socket = Some(Socket {
                    ws,
                    _open: open,
                    _close: close,
                    _message: message,
                });
            }
            Err(_) => ctx.link().send_message(Msg::Closed(epoch)),
        }
    }
    fn send(&mut self, message: ClientMessage) {
        if !self.connected {
            self.error = Some("Reconnecting to the table. Your seat is saved.".into());
            return;
        }
        if let Some(socket) = &self.socket {
            if socket
                .ws
                .send_with_str(&serde_json::to_string(&message).unwrap())
                .is_err()
            {
                self.error = Some("Could not send the move. Reconnecting…".into());
            } else if message != ClientMessage::Ping {
                self.busy = true;
                self.error = None;
            }
        }
    }
    fn enable_audio(&mut self) {
        if self.sound && self.audio.is_none() {
            self.audio = web_sys::AudioContext::new().ok();
        }
        if let Some(a) = &self.audio {
            let _ = a.resume();
        }
    }
    fn chime(&self) {
        if !self.sound {
            return;
        }
        if let Some(a) = &self.audio {
            if let (Ok(o), Ok(g)) = (a.create_oscillator(), a.create_gain()) {
                o.set_type(web_sys::OscillatorType::Sine);
                o.frequency().set_value(660.0);
                g.gain().set_value(0.045);
                let _ = g
                    .gain()
                    .exponential_ramp_to_value_at_time(0.001, a.current_time() + 0.15);
                let _ = o.connect_with_audio_node(&g);
                let _ = g.connect_with_audio_node(&a.destination());
                let _ = o.start();
                let _ = o.stop_with_when(a.current_time() + 0.18);
            }
        }
    }
    fn action(
        &self,
        ctx: &Context<Self>,
        text: &str,
        message: ClientMessage,
        class: &str,
        disabled: bool,
    ) -> Html {
        html! { <button class={class.to_string()} disabled={disabled || !self.connected || self.busy} onclick={ctx.link().callback(move |_| Msg::Send(message.clone()))}>{text}</button> }
    }
    fn card(card: Card, class: &str) -> Html {
        html! { <img class={classes!("card-image", class.to_string())} src={card.asset()} alt={format!("{} of {}", card.rank.label(), card.suit.name())} draggable="false"/> }
    }
    fn player_name(room: &RoomView, seat: usize) -> String {
        room.seats[seat]
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Empty seat".into())
    }
    fn header(&self, ctx: &Context<Self>) -> Html {
        html! {
            <header class="header">
                <button class="brand" onclick={ctx.link().callback(|_| Msg::Page(Page::Menu))} aria-label="Belote Club home"><span class="brand-mark">{"♠"}</span><span>{"BELOTE"}<small>{"THE GOOD COMPANY CLUB"}</small></span></button>
                <nav>
                    <span class={classes!("connection", if self.connected {"online"} else {"offline"})}><i/>{if self.connected {"Connected"} else {"Connecting…"}}</span>
                    <button class="text-button rules-nav" onclick={ctx.link().callback(|_| Msg::Modal(Some(Modal::Rules)))}>{"How to play"}</button>
                    <button class="icon-button" aria-label="Settings" title="Settings" onclick={ctx.link().callback(|_| Msg::Modal(Some(Modal::Settings)))}>{"⚙"}</button>
                </nav>
            </header>
        }
    }
    fn menu(&self, ctx: &Context<Self>) -> Html {
        html! {
            <main class="home">
                <section class="hero-copy">
                    <div class="eyebrow"><span/>{"A CLASSIC GAME. YOUR PEOPLE."}</div>
                    <h1>{"Good cards."}<br/><em>{"Better company."}</em></h1>
                    <p class="hero-description">{"An evening around the table, wherever you are. Bring your friends. We’ll deal the cards."}</p>
                    <button class="primary main-cta" onclick={ctx.link().callback(|_| Msg::Page(Page::Lobby))}><span>{"New game"}</span><span>{"↗"}</span></button>
                    <button class="secondary-menu" onclick={ctx.link().callback(|_| Msg::Modal(Some(Modal::Settings)))}>{"Settings"}<span>{"Sound & table theme"}</span></button>
                    <div class="home-details"><span>{"♧"}{" 4 players"}</span><span>{"32 cards"}</span><span>{"Bulgarian belot"}</span></div>
                </section>
                <section class="hero-art" aria-label="A hand of playing cards on green felt">
                    <div class="art-orbit orbit-one"/><div class="art-orbit orbit-two"/>
                    <div class="hero-stamp"><span>{"EST. FOR FRIENDS"}</span><b>{"♠"}</b><span>{"THE TABLE IS YOURS"}</span></div>
                    <div class="hero-fan">
                        {Self::card(Card::new(Suit::Clubs, Rank::Jack), "hero-card hero-card-1")}
                        {Self::card(Card::new(Suit::Diamonds, Rank::Queen), "hero-card hero-card-2")}
                        {Self::card(Card::new(Suit::Hearts, Rank::King), "hero-card hero-card-3")}
                        {Self::card(Card::new(Suit::Spades, Rank::Ace), "hero-card hero-card-4")}
                    </div>
                    <div class="art-caption"><span>{"01 /"}</span>{" The original green table"}</div>
                </section>
            </main>
        }
    }
    fn lobby(&self, ctx: &Context<Self>) -> Html {
        html! {
            <main class="lobby-page">
                <section class="lobby-intro"><button class="back" onclick={ctx.link().callback(|_| Msg::Page(Page::Menu))}>{"←  Main menu"}</button><div class="eyebrow">{"GOOD EVENINGS START HERE"}</div><h1>{"Pull up"}<br/><em>{"a chair."}</em></h1><p>{"Create a private table and invite your people, or join the one they’ve saved for you."}</p><div class="little-suits">{"♣  ♦  ♥  ♠"}</div><small>{"Two teams. First to 151. Always better together."}</small></section>
                <section class="lobby-panel panel">
                    <label for="player-name">{"WHAT SHOULD WE CALL YOU?"}</label>
                    <input id="player-name" maxlength="24" placeholder="Your name" autocomplete="nickname" value={self.name.clone()} oninput={ctx.link().callback(|e: InputEvent| Msg::Name(e.target_unchecked_into::<HtmlInputElement>().value()))}/>
                    <div class="lobby-option"><span class="option-number">{"01"}</span><div><h2>{"Start your own table"}</h2><p>{"A private room for you and three friends."}</p></div></div>
                    <button class="primary full" disabled={!self.connected || self.busy || self.name.trim().is_empty()} onclick={ctx.link().callback(|_| Msg::Create)}>{"Create a table"}<span>{"+"}</span></button>
                    <div class="divider"><span>{"OR TAKE YOUR SEAT"}</span></div>
                    <label for="room-code">{"HAVE AN INVITE CODE?"}</label>
                    <div class="join-row"><input id="room-code" class="code-input" placeholder="ABC123" maxlength="6" value={self.code.clone()} oninput={ctx.link().callback(|e: InputEvent| Msg::Code(e.target_unchecked_into::<HtmlInputElement>().value()))}/><button class="outline" disabled={!self.connected || self.busy || self.name.trim().is_empty() || self.code.len() != 6} onclick={ctx.link().callback(|_| Msg::Join)}>{"Join table →"}</button></div>
                    <p class="panel-footnote">{"No account needed. Just a name and good company."}</p>
                </section>
            </main>
        }
    }
    fn waiting(&self, ctx: &Context<Self>, room: &RoomView) -> Html {
        let count = room.seats.iter().flatten().count();
        let ready = room
            .seats
            .iter()
            .all(|s| s.as_ref().is_some_and(|p| p.connected));
        html! {
            <main class="waiting-page">
                <div class="page-title"><div><div class="eyebrow">{"THE TABLE IS SET"}</div><h1>{"Room for "}<em>{"good company."}</em></h1><p>{"Partners sit opposite each other. Choose a seat before the first deal."}</p></div><button class="text-button" onclick={ctx.link().callback(|_| Msg::Modal(Some(Modal::Leave)))}>{"Leave table ↗"}</button></div>
                <div class="waiting-layout">
                    <section class="seats-panel panel">
                        <div class="section-label"><span>{"YOUR PLAYERS"}</span><span>{format!("{count} / 4 seated")}</span></div>
                        <div class="teams-grid">{for (0..2).map(|team| html! {
                            <div class="team-column"><h3>{if team == room.you % 2 {"YOUR TEAM"} else {"OTHER TEAM"}}</h3>{for [team, team + 2].into_iter().map(|seat| self.lobby_seat(ctx, room, seat))}</div>
                        })}</div>
                        <div class="waiting-bottom"><span class="small-dot"/>{if ready {"Everyone’s here. Let’s play."} else {"Waiting for your people. Invite friends or add a bot."}}</div>
                    </section>
                    <aside class="invite-panel panel"><span class="eyebrow">{"SAVE THEM A SEAT"}</span><h2>{"Better with friends."}</h2><p>{"Share this code, or send an invitation link."}</p><div class="invite-code">{&room.code}</div><button class="outline full" onclick={ctx.link().callback(|_| Msg::CopyInvite)}>{"Copy invitation link ↗"}</button><div class="room-settings"><span>{"RULES"}</span><b>{"Bulgarian belot"}</b><span>{"PLAYING TO"}</span><b>{"151 points"}</b><span>{"TABLE"}</span><b>{"Classic green"}</b></div>
                    {if room.host == room.you {self.action(ctx,"Deal the cards →", ClientMessage::Start,"primary full", !ready)} else {html! {<p class="muted">{format!("{} will start the game.", Self::player_name(room, room.host))}</p>}}}<small>{"Missing a friend? The host can add practice bots."}</small></aside>
                </div>
            </main>
        }
    }
    fn lobby_seat(&self, ctx: &Context<Self>, room: &RoomView, seat: usize) -> Html {
        html! { <div class={classes!("lobby-seat", if room.you == seat {"my-seat"} else {""})}>
            {if let Some(p) = &room.seats[seat] { html! {
                <><div class="avatar">{p.name.chars().next().unwrap_or('?').to_uppercase().to_string()}</div><div class="seat-name"><b>{&p.name}{if seat == room.you {" (you)"} else {""}}</b><small>{if p.bot {"Practice bot"} else if !p.connected {"Reconnecting…"} else if room.host == seat {"Table host"} else {"Ready to play"}}</small></div>
                {if p.bot && room.host == room.you { self.action(ctx, "×", ClientMessage::RemoveBot { seat }, "icon-button", false) } else if !p.connected && room.host == room.you {self.action(ctx,"Use bot",ClientMessage::AddBot {seat},"text-button",false)} else {html! {<span class="ready-tick">{if p.connected {"✓"} else {"…"}}</span>}}}</>
            }} else { html! {
                <><div class="avatar empty">{"+"}</div><div class="seat-name"><b>{"An open chair"}</b>{self.action(ctx, "Sit here", ClientMessage::Sit {seat}, "text-button", false)}</div>{if room.host == room.you {self.action(ctx, "+ Bot", ClientMessage::AddBot {seat}, "small-button", false)} else {html!{}}}</>
            }}}
        </div> }
    }
    fn table(&self, ctx: &Context<Self>, room: &RoomView, g: &GameView) -> Html {
        let your_team = room.you % 2;
        let is_turn = g.turn == room.you && matches!(g.phase, Phase::Bidding | Phase::Playing);
        let status = match g.phase {
            Phase::Bidding => {
                if is_turn {
                    "Your call. Choose a contract.".into()
                } else {
                    format!("{} is bidding…", Self::player_name(room, g.turn))
                }
            }
            Phase::Playing => {
                if is_turn {
                    "Your turn. Play a highlighted card.".into()
                } else {
                    format!("{} is thinking…", Self::player_name(room, g.turn))
                }
            }
            Phase::TrickEnd => format!(
                "{} takes the trick",
                Self::player_name(
                    room,
                    trick_winner(&g.trick, g.contract.unwrap().contract)
                        .unwrap()
                        .seat
                )
            ),
            _ => "The hand is complete.".into(),
        };
        html! {
            <main class="game-page">
                <div class="game-toolbar"><div class="table-label"><span class="eyebrow">{format!("TABLE {}", room.code)}</span><span>{format!("Hand {:02} · Bulgarian belot",g.round)}</span></div>
                    <button class="score-strip" onclick={ctx.link().callback(|_| Msg::Modal(Some(Modal::Scores)))}><span>{"US"}<b>{g.scores[your_team]}</b></span><span class="score-target">{"151"}<small>{"TO WIN"}</small></span><span><b>{g.scores[1-your_team]}</b>{"THEM"}</span></button>
                    <button class="text-button" onclick={ctx.link().callback(|_| Msg::Modal(Some(Modal::Leave)))}>{"Leave table ↗"}</button>
                </div>
                {for room.seats.iter().enumerate().filter(|(_,p)| p.as_ref().is_some_and(|p|!p.connected)).map(|(seat,_)| html!{<div class="offline-banner">{format!("{} is disconnected. Their seat is saved.",Self::player_name(room,seat))}{if room.you==room.host {self.action(ctx,"Replace with bot",ClientMessage::AddBot{seat},"text-button",false)}else{html!{}}}</div>})}
                <div class="table-stage">
                    <div class="felt-table"><div class="table-inner-line"/><div class="table-watermark">{"♠"}<span>{"BELOTE CLUB"}</span></div></div>
                    {for (0..4).map(|offset| self.table_player(room,g,(room.you+offset)%4,offset))}
                    <div class="table-center">
                        {if g.phase == Phase::Bidding {self.bidding(ctx,room,g)} else if matches!(g.phase,Phase::RoundEnd|Phase::Redeal|Phase::MatchEnd) {self.round_end(ctx,room,g)} else {html!{
                            <><div class="contract-pill">{g.contract.map(|b|format!("{} {}{}",b.contract.symbol(),b.contract.label(),if b.multiplier>1 {format!(" ×{}",b.multiplier)}else{String::new()})).unwrap_or_default()}</div><div class="trick-cards">{for g.trick.iter().map(|p|html!{<div class={format!("played-card played-{}",(p.seat+4-room.you)%4)}>{Self::card(p.card,"")}</div>})}</div>
                            {if g.trick.is_empty() {html!{<span class="lead-hint">{"Lead the next trick"}</span>}} else {html!{}}}</>
                        }}}
                    </div>
                </div>
                <section class="hand-area">
                    <div class={classes!("turn-status", if is_turn {"your-turn"}else{""})}><i/>{status}</div>
                    {if !g.declarations.is_empty() {html!{<div class="declaration-picker"><span>{"DECLARE WITH YOUR FIRST CARD"}</span>{for g.declarations.iter().enumerate().map(|(i,d)|html!{<button class={classes!("declaration",if self.selected.contains(&i){"selected"}else{""})} aria-pressed={self.selected.contains(&i).to_string()} onclick={ctx.link().callback(move |_|Msg::SelectDeclaration(i))}>{if self.selected.contains(&i){"✓ "}else{"+ "}}{&d.label}</button>})}</div>}}else{html!{}}}
                    <div class="hand" aria-label="Your cards">{for g.hand.iter().enumerate().map(|(i,card)| {
                        let card=*card; let legal=g.legal_cards.contains(&card); let angle=(i as f64 - (g.hand.len() as f64-1.0)/2.0)*2.8;
                        html!{<button class={classes!("hand-card",if legal {"playable"}else{""},if g.phase==Phase::Playing && is_turn && !legal {"illegal"}else{""})} style={format!("--angle:{angle}deg;--lift:{}px",angle.abs()*0.7)} disabled={!legal || self.busy || !self.connected} aria-label={format!("Play {} of {}",card.rank.label(),card.suit.name())} title={format!("{} of {}",card.rank.label(),card.suit.name())} onclick={ctx.link().callback(move |_|Msg::Play(card))}>{Self::card(card,"")}</button>}
                    })}</div>
                </section>
                <div class="game-bottom"><span>{format!("TRICKS  {} — {}",g.tricks[your_team],g.tricks[1-your_team])}</span><span class="latest-announcement">{g.announcements.last().cloned().unwrap_or_else(||"Partners across the table. Play moves to your right.".into())}</span>
                    <details class="last-trick"><summary>{"Last trick ↗"}</summary><div>{if g.last_trick.is_empty(){html!{<p>{"No tricks played yet."}</p>}}else{html!{<><p>{format!("Won by {}",Self::player_name(room,g.last_winner.unwrap()))}</p>{for g.last_trick.iter().map(|p|Self::card(p.card,""))}</>}}}</div></details>
                </div>
            </main>
        }
    }
    fn table_player(&self, room: &RoomView, g: &GameView, seat: usize, offset: usize) -> Html {
        let p = room.seats[seat].as_ref().unwrap();
        let active = g.turn == seat && matches!(g.phase, Phase::Bidding | Phase::Playing);
        let bid = g.bids.iter().rev().find(|b| b.seat == seat);
        html! {<div class={classes!("table-player",format!("player-{offset}"),if active{"active"}else{""})}>
            {if offset>0 {html!{<div class="hidden-hand">{for (0..g.hand_counts[seat]).map(|_|html!{<img src="/assets/cards/back.png" alt="Face-down card"/>})}</div>}}else{html!{}}}
            <div class="player-plate"><div class="avatar">{p.name.chars().next().unwrap_or('?').to_uppercase().to_string()}</div><div><b>{&p.name}{if offset==0{" (you)"}else{""}}</b><small>{if !p.connected {"Reconnecting"}else if offset==2 {"Your partner"}else if offset==0 {"Your hand"}else if p.bot {"Practice bot"}else{"Opponent"}}</small></div>{if g.dealer==seat{html!{<span class="dealer-chip" title="Dealer">{"D"}</span>}}else{html!{}}}</div>
            {if g.phase==Phase::Bidding {bid.map(|b|html!{<span class="bid-bubble">{b.bid.label()}</span>}).unwrap_or_default()}else{html!{}}}
        </div>}
    }
    fn bidding(&self, ctx: &Context<Self>, room: &RoomView, g: &GameView) -> Html {
        html! {<div class="bidding-panel"><span class="eyebrow">{"THE BIDDING"}</span><h2>{if g.turn==room.you{"Make your call"}else{"A little friendly bidding"}}</h2>
        <p>{g.contract.map(|b|format!("{} called {}{}",Self::player_name(room,b.bidder),b.contract.label(),if b.multiplier>1{format!(" ×{}",b.multiplier)}else{String::new()})).unwrap_or_else(||"Pick a suit, no trumps, or all trumps.".into())}</p>
        <div class="bid-grid">{for Contract::ALL.map(|contract| {
            let bid=Bid::Contract(contract);
            html!{<button class={classes!("bid-choice",if matches!(contract,Contract::Hearts|Contract::Diamonds){"red-suit"}else{""})} disabled={!g.legal_bids.contains(&bid)||self.busy||!self.connected} onclick={ctx.link().callback(move |_|Msg::Send(ClientMessage::Bid{bid}))}><b>{contract.symbol()}</b><span>{contract.label()}</span></button>}
        })}</div><div class="bid-actions">{for [Bid::Pass,Bid::Double,Bid::Redouble].into_iter().filter(|b|*b==Bid::Pass||g.legal_bids.contains(b)).map(|bid|self.action(ctx,bid.label(),ClientMessage::Bid{bid},if bid==Bid::Pass{"outline"}else{"primary"},!g.legal_bids.contains(&bid)))}</div></div>}
    }
    fn round_end(&self, ctx: &Context<Self>, room: &RoomView, g: &GameView) -> Html {
        let title = if g.phase == Phase::Redeal {
            "Everyone passed"
        } else if g.phase == Phase::MatchEnd {
            if g.winner == Some(room.you % 2) {
                "The table is yours!"
            } else {
                "Well played, everyone."
            }
        } else {
            "That’s a hand."
        };
        html! {<div class="round-panel"><span class="eyebrow">{if g.phase==Phase::MatchEnd{"MATCH COMPLETE"}else{"CARDS ON THE TABLE"}}</span><h2>{title}</h2>
            {if g.phase==Phase::Redeal{html!{<p>{"The next dealer will shuffle and deal again."}</p>}}else{g.history.last().map(|r|html!{<><p>{&r.outcome}{if r.capot{" · Capot! One more hand to go out."}else{""}}</p><div class="round-scores"><div><small>{"YOUR TEAM"}</small><b>{format!("+{}",r.earned[room.you%2])}</b><span>{format!("{} card points",r.points[room.you%2])}</span></div><div><small>{"THEIR TEAM"}</small><b>{format!("+{}",r.earned[1-room.you%2])}</b><span>{format!("{} card points",r.points[1-room.you%2])}</span></div></div></>}).unwrap_or_default()}}
            {if g.hanging>0 {html!{<p>{format!("{} points carry into the next hand.",g.hanging)}</p>}}else{html!{}}}
            {if room.host==room.you{self.action(ctx,if g.phase==Phase::MatchEnd{"Play again →"}else{"Next hand →"},ClientMessage::NextRound,"primary full",false)}else{html!{<p>{format!("Waiting for {} to deal…",Self::player_name(room,room.host))}</p>}}}
        </div>}
    }
    fn modal_view(&self, ctx: &Context<Self>, modal: Modal) -> Html {
        html! {<div class="modal-backdrop" onclick={ctx.link().callback(|_|Msg::Modal(None))}><section class="modal panel" role="dialog" aria-modal="true" aria-label={match modal{Modal::Settings=>"Settings",Modal::Rules=>"How to play",Modal::Scores=>"Scorebook",Modal::Leave=>"Leave table"}} onclick={Callback::from(|e:MouseEvent|e.stop_propagation())}>
            <button class="modal-close icon-button" aria-label="Close dialog" onclick={ctx.link().callback(|_|Msg::Modal(None))}>{"×"}</button>
            {match modal {
                Modal::Settings=>html!{<><div class="eyebrow">{"MAKE YOURSELF AT HOME"}</div><h2>{"A table to your taste."}</h2><div class="setting-row"><div><b>{"Game sounds"}</b><p>{"A gentle note when it’s your turn."}</p></div><button class={classes!("toggle",if self.sound{"enabled"}else{""})} role="switch" aria-checked={self.sound.to_string()} aria-label="Game sounds" onclick={ctx.link().callback(|_|Msg::Sound)}><span/></button></div><div class="theme-preview"><div class="theme-swatch">{"♠"}</div><div><b>{"Classic green"}</b><p>{"Soft felt. Timeless cards."}</p></div><span class="theme-check">{"✓"}</span></div><small>{"One good table is all you need. More themes can come later."}</small></>},
                Modal::Rules=>html!{<><div class="eyebrow">{"A QUICK REFRESHER"}</div><h2>{"Welcome to belot."}</h2><div class="rules-content"><p>{"Four players, two partnerships, 32 cards. Partners sit opposite. Play moves counterclockwise. First team to 151 wins; you cannot go out on a capot."}</p><h3>{"01 · Call the game"}</h3><p>{"Bid with five cards: clubs < diamonds < hearts < spades < no trumps < all trumps. Three passes end bidding; everyone gets three more cards. Opponents can double, and the declaring team can redouble."}</p><h3>{"02 · Follow the cards"}</h3><p>{"Follow suit. Raise when following trump or in all trumps. If you cannot follow, trump an opponent’s winning card if possible; if you cannot overtrump, discard freely. No trumps only requires following suit."}</p><div class="rank-guide"><span>{"TRUMP"}</span><b>{"J 9 A 10 K Q 8 7"}</b><span>{"PLAIN"}</span><b>{"A 10 K Q J 9 8 7"}</b></div><h3>{"03 · Make it count"}</h3><p>{"Select sequences or four-of-a-kind before your first card. Overlapping declarations cannot both count. Belote (the king and queen of a trump suit) is announced automatically on the first eligible card. No declarations in no trumps."}</p><p>{"The last trick adds 10; all eight tricks add 90. No-trump card points are doubled. The bidding team must beat the other team or surrender the hand’s points. Double and redouble award the whole pot to the winning team. Tied points carry forward."}</p><p>{"Highlighting shows legal cards. The scorebook keeps every hand. Bots are simple practice partners."}</p></div></>},
                Modal::Scores=>self.scorebook(),
                Modal::Leave=>html!{<><div class="eyebrow">{"UNTIL NEXT TIME"}</div><h2>{"Leave this table?"}</h2><p>{if self.room.as_ref().is_some_and(|r|r.game.is_some()){"Your seat will be marked offline. The host can replace it with a bot so the others can continue."}else{"Your chair will be free for another friend."}}</p><div class="modal-actions"><button class="outline" onclick={ctx.link().callback(|_|Msg::Modal(None))}>{"Stay here"}</button>{self.action(ctx,"Leave table",ClientMessage::Leave,"primary",false)}</div></>},
            }}
        </section></div>}
    }
    fn scorebook(&self) -> Html {
        let Some(room) = &self.room else {
            return html! {};
        };
        let Some(g) = &room.game else { return html! {} };
        let team = room.you % 2;
        html! {<><div class="eyebrow">{"EVERY HAND COUNTS"}</div><h2>{"The scorebook."}</h2><div class="scorebook-scroll"><table><thead><tr><th>{"HAND"}</th><th>{"CONTRACT"}</th><th>{"US"}</th><th>{"THEM"}</th></tr></thead><tbody>{for g.history.iter().map(|r|html!{<tr><td>{r.round}</td><td>{format!("{} ×{}",r.contract.contract.label(),r.contract.multiplier)}</td><td>{format!("+{}",r.earned[team])}</td><td>{format!("+{}",r.earned[1-team])}</td></tr>})}</tbody><tfoot><tr><th colspan="2">{"TOTAL"}</th><td>{g.scores[team]}</td><td>{g.scores[1-team]}</td></tr></tfoot></table></div><p>{format!("{} points carried forward · First to 151",g.hanging)}</p><div class="announcement-list">{for g.announcements.iter().map(|a|html!{<p>{a}</p>})}</div></>}
    }
}

impl Component for App {
    type Message = Msg;
    type Properties = ();
    fn create(ctx: &Context<Self>) -> Self {
        let query = window().location().search().unwrap_or_default();
        let code = query
            .trim_start_matches('?')
            .split('&')
            .find_map(|p| p.strip_prefix("room="))
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(6)
            .collect::<String>()
            .to_uppercase();
        let link = ctx.link().clone();
        let mut app = Self {
            socket: None,
            epoch: 0,
            connected: false,
            retry: None,
            retries: 0,
            _heartbeat: Interval::new(20000, move || link.send_message(Msg::Heartbeat)),
            page: if code.is_empty() {
                Page::Menu
            } else {
                Page::Lobby
            },
            modal: None,
            name: stored("belote.name", false).unwrap_or_default(),
            code,
            room: None,
            error: None,
            notice: None,
            busy: false,
            selected: vec![],
            sound: stored("belote.sound", false).as_deref() != Some("off"),
            audio: None,
        };
        app.connect(ctx);
        app
    }
    fn update(&mut self, ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::Connect => self.connect(ctx),
            Msg::Opened(epoch) => {
                if epoch != self.epoch {
                    return false;
                }
                self.connected = true;
                self.retries = 0;
                self.error = None;
                if let Some(saved) = stored("belote.session", true)
                    .and_then(|s| serde_json::from_str::<(String, String)>(&s).ok())
                {
                    self.send(ClientMessage::Resume {
                        code: saved.0,
                        token: saved.1,
                    });
                }
            }
            Msg::Closed(epoch) => {
                if epoch != self.epoch || self.retry.is_some() {
                    return false;
                }
                self.connected = false;
                self.busy = false;
                self.retries += 1;
                let delay = (1000 * self.retries).min(10000);
                let link = ctx.link().clone();
                self.retry = Some(Timeout::new(delay, move || link.send_message(Msg::Connect)));
            }
            Msg::Received(epoch, msg) => {
                if epoch != self.epoch {
                    return false;
                }
                match msg {
                    ServerMessage::Session { code, token } => save(
                        "belote.session",
                        &serde_json::to_string(&(code, token)).unwrap(),
                        true,
                    ),
                    ServerMessage::State { room } => {
                        let old = self.room.as_ref().and_then(|r| r.game.as_ref());
                        let new = room.game.as_ref();
                        if self.room.is_none() || old.is_none() && new.is_some() {
                            window().scroll_to_with_x_and_y(0.0, 0.0);
                        }
                        if old.map(|g| &g.declarations) != new.map(|g| &g.declarations) {
                            self.selected = new
                                .map(|g| best_declarations(&g.declarations))
                                .unwrap_or_default();
                        }
                        let turn = new.is_some_and(|g| {
                            g.turn == room.you && matches!(g.phase, Phase::Bidding | Phase::Playing)
                        });
                        if turn
                            && old.is_none_or(|g| {
                                g.turn != room.you
                                    || new.is_some_and(|n| n.phase != g.phase || n.round != g.round)
                            })
                        {
                            self.chime();
                        }
                        self.room = Some(*room);
                        self.busy = false;
                    }
                    ServerMessage::Error { message } => {
                        if message.starts_with("Your saved seat") {
                            clear_session();
                            self.room = None;
                            self.page = Page::Lobby;
                        }
                        self.error = Some(message);
                        self.busy = false;
                    }
                    ServerMessage::Left => {
                        clear_session();
                        self.room = None;
                        self.page = Page::Lobby;
                        self.modal = None;
                        self.busy = false;
                    }
                    ServerMessage::Pong => return false,
                }
            }
            Msg::Page(page) => {
                if self.room.is_some() {
                    self.modal = Some(Modal::Leave);
                } else {
                    self.page = page;
                    self.error = None;
                    window().scroll_to_with_x_and_y(0.0, 0.0);
                }
                self.enable_audio();
            }
            Msg::Modal(modal) => self.modal = modal,
            Msg::Name(name) => {
                self.name = name;
                save("belote.name", &self.name, false);
            }
            Msg::Code(code) => {
                self.code = code
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .take(6)
                    .collect::<String>()
                    .to_uppercase()
            }
            Msg::Create => {
                self.enable_audio();
                self.send(ClientMessage::Create {
                    name: self.name.clone(),
                });
            }
            Msg::Join => {
                self.enable_audio();
                self.send(ClientMessage::Join {
                    name: self.name.clone(),
                    code: self.code.clone(),
                });
            }
            Msg::Send(msg) => {
                self.enable_audio();
                self.send(msg);
            }
            Msg::Play(card) => {
                self.send(ClientMessage::Play {
                    card,
                    declarations: self.selected.clone(),
                });
            }
            Msg::SelectDeclaration(i) => {
                if self.selected.contains(&i) {
                    self.selected.retain(|v| *v != i);
                } else if let Some(g) = self.room.as_ref().and_then(|r| r.game.as_ref()) {
                    if let Some(option) = g.declarations.get(i) {
                        self.selected
                            .retain(|j| !g.declarations[*j].conflicts_with(option));
                        self.selected.push(i);
                    }
                }
            }
            Msg::Sound => {
                self.sound = !self.sound;
                save("belote.sound", if self.sound { "on" } else { "off" }, false);
                self.enable_audio();
                self.chime();
            }
            Msg::CopyInvite => {
                if let Some(room) = &self.room {
                    let url = format!(
                        "{}/?room={}",
                        window().location().origin().unwrap_or_default(),
                        room.code
                    );
                    let link = ctx.link().clone();
                    if window().is_secure_context() {
                        let promise = window().navigator().clipboard().write_text(&url);
                        yew::platform::spawn_local(async move {
                            let message =
                                if wasm_bindgen_futures::JsFuture::from(promise).await.is_ok() {
                                    "Invitation link copied.".into()
                                } else {
                                    format!("Copy this invitation: {url}")
                                };
                            link.send_message(Msg::Notice(message));
                        });
                    } else {
                        self.notice = Some(format!("Copy this invitation: {url}"));
                    }
                }
            }
            Msg::Notice(message) => self.notice = Some(message),
            Msg::Dismiss => {
                self.error = None;
                self.notice = None;
            }
            Msg::Heartbeat => {
                if self.connected {
                    self.send(ClientMessage::Ping);
                }
                return false;
            }
        }
        true
    }
    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {<div class={classes!("app",if self.room.as_ref().is_some_and(|r|r.game.is_some()){"in-game"}else{""})}>
            {self.header(ctx)}
            {if let Some(room)=&self.room{if let Some(game)=&room.game{self.table(ctx,room,game)}else{self.waiting(ctx,room)}}else{match self.page{Page::Menu=>self.menu(ctx),Page::Lobby=>self.lobby(ctx)}}}
            {if self.room.as_ref().is_none_or(|r|r.game.is_none()){html!{<footer><span>{"MADE FOR A GOOD EVENING"}</span><div class="footer-suits">{"♣"}<span>{"♦  ♥"}</span>{"♠"}</div><span>{"32 CARDS. ENDLESS STORIES."}</span></footer>}}else{html!{}}}
            {if !self.connected && self.room.is_some(){html!{<div class="reconnecting" role="status">{"Connection lost. Reconnecting to your saved seat…"}</div>}}else{html!{}}}
            {if let Some(message)=self.error.as_ref().or(self.notice.as_ref()){html!{<div class={classes!("toast",if self.error.is_some(){"error"}else{""})} role="alert"><span>{message}</span><button class="icon-button" aria-label="Dismiss notification" onclick={ctx.link().callback(|_|Msg::Dismiss)}>{"×"}</button></div>}}else{html!{}}}
            {self.modal.map(|m|self.modal_view(ctx,m)).unwrap_or_default()}
        </div>}
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    yew::Renderer::<App>::new().render();
}
