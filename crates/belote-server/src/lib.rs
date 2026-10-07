use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use belote_core::*;
use futures_util::{SinkExt, StreamExt};
use rand::{seq::SliceRandom, Rng};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex, Semaphore};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

type Outbox = mpsc::Sender<ServerMessage>;
type Session = Option<(String, String)>;

struct Player {
    name: String,
    token: String,
    outbox: Option<Outbox>,
    connection: u64,
    bot: bool,
}
impl Player {
    fn view(&self) -> PlayerView {
        PlayerView {
            name: self.name.clone(),
            connected: self.outbox.as_ref().is_some_and(|o| !o.is_closed()) || self.bot,
            bot: self.bot,
        }
    }
}

struct Room {
    code: String,
    seats: [Option<Player>; 4],
    host: usize,
    game: Option<Game>,
    due: Instant,
    touched: Instant,
}
impl Room {
    fn broadcast(&self) {
        for (you, seat) in self.seats.iter().enumerate() {
            if let Some(outbox) = seat.as_ref().and_then(|p| p.outbox.as_ref()) {
                let room = RoomView {
                    code: self.code.clone(),
                    seats: self.seats.each_ref().map(|p| p.as_ref().map(Player::view)),
                    you,
                    host: self.host,
                    game: self.game.as_ref().map(|g| g.view(you)),
                };
                let _ = outbox.try_send(ServerMessage::State {
                    room: Box::new(room),
                });
            }
        }
    }
    fn transfer_host(&mut self) {
        if self.seats[self.host]
            .as_ref()
            .is_none_or(|p| p.outbox.is_none())
        {
            if let Some(host) = self
                .seats
                .iter()
                .position(|p| p.as_ref().is_some_and(|p| p.outbox.is_some() && !p.bot))
            {
                self.host = host;
            }
        }
    }
    fn schedule(&mut self) {
        self.touched = Instant::now();
        let millis = if self
            .game
            .as_ref()
            .is_some_and(|g| g.phase == Phase::TrickEnd)
        {
            1500
        } else {
            850
        };
        self.due = Instant::now() + Duration::from_millis(millis);
    }
}

#[derive(Clone)]
struct AppState {
    rooms: Arc<Mutex<HashMap<String, Room>>>,
    connections: Arc<Semaphore>,
}

pub fn app(web: PathBuf) -> (Router, tokio::task::JoinHandle<()>) {
    let state = AppState {
        rooms: Default::default(),
        connections: Arc::new(Semaphore::new(512)),
    };
    let maintenance = tokio::spawn(maintain(state.clone()));
    let router = Router::new()
        .route("/ws", get(upgrade))
        .route("/health", get(|| async { "ok" }))
        .fallback_service(ServeDir::new(web).append_index_html_on_directories(true))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            header::HeaderValue::from_static("nosniff"),
        ))
        .with_state(state);
    (router, maintenance)
}

async fn upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Browser requests must originate from the site serving this server. Native clients may omit Origin.
    if let Some(origin) = headers.get(header::ORIGIN) {
        let host = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let origin = origin.to_str().unwrap_or("");
        if origin != format!("http://{host}") && origin != format!("https://{host}") {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let Ok(permit) = state.connections.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    ws.max_message_size(8192)
        .max_frame_size(8192)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            connected(socket, state).await;
        })
}

async fn connected(socket: WebSocket, state: AppState) {
    let (mut writer, mut reader) = socket.split();
    let (outbox, mut outgoing) = mpsc::channel(128);
    let connection = rand::random();
    let mut session = None;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    let mut last_seen = Instant::now();
    let mut rate_window = Instant::now();
    let mut messages = 0;
    loop {
        tokio::select! {
            msg = outgoing.recv() => {
                let Some(msg) = msg else { break };
                let json = serde_json::to_string(&msg).expect("serializable protocol");
                if !matches!(tokio::time::timeout(Duration::from_secs(5), writer.send(Message::Text(json.into()))).await, Ok(Ok(()))) { break; }
            }
            frame = reader.next() => {
                let Some(Ok(frame)) = frame else { break };
                last_seen = Instant::now();
                match frame {
                    Message::Text(text) => {
                        if rate_window.elapsed() >= Duration::from_secs(1) { rate_window = Instant::now(); messages = 0; }
                        messages += 1;
                        if messages > 30 { break; }
                        let result = match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(message) => handle(&state, &mut session, connection, &outbox, message).await,
                            Err(_) => Err("Unrecognized message.".into()),
                        };
                        if let Err(message) = result { let _ = outbox.try_send(ServerMessage::Error { message }); }
                    }
                    Message::Ping(data) => { if writer.send(Message::Pong(data)).await.is_err() { break; } }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = heartbeat.tick() => {
                if last_seen.elapsed() > Duration::from_secs(65) || writer.send(Message::Ping(vec![].into())).await.is_err() { break; }
            }
        }
    }
    detach(&state, session, connection, false).await;
}

fn clean_name(name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 24 || name.chars().any(char::is_control) {
        return Err("Choose a name between 1 and 24 characters.".into());
    }
    Ok(name.to_string())
}
fn code() -> String {
    const LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| LETTERS[rng.gen_range(0..LETTERS.len())] as char)
        .collect()
}
fn player(name: String, connection: u64, outbox: &Outbox) -> Player {
    Player {
        name,
        token: format!("{:032x}", rand::random::<u128>()),
        connection,
        outbox: Some(outbox.clone()),
        bot: false,
    }
}
fn shuffled_deck() -> Vec<Card> {
    let mut cards = deck();
    cards.shuffle(&mut rand::thread_rng());
    cards
}

async fn handle(
    state: &AppState,
    session: &mut Session,
    connection: u64,
    outbox: &Outbox,
    message: ClientMessage,
) -> Result<(), String> {
    if message == ClientMessage::Ping {
        let _ = outbox.try_send(ServerMessage::Pong);
        return Ok(());
    }
    if message == ClientMessage::Leave {
        detach(state, session.take(), connection, true).await;
        let _ = outbox.try_send(ServerMessage::Left);
        return Ok(());
    }
    let mut rooms = state.rooms.lock().await;
    if matches!(
        message,
        ClientMessage::Create { .. } | ClientMessage::Join { .. } | ClientMessage::Resume { .. }
    ) {
        if session.is_some() {
            return Err("Leave your current table before joining another.".into());
        }
        let (code, seat) = match message {
            ClientMessage::Create { name } => {
                let name = clean_name(name)?;
                if rooms.len() >= 256 {
                    return Err("The server is full. Try again later.".into());
                }
                let mut id = code();
                while rooms.contains_key(&id) {
                    id = code();
                }
                let mut room = Room {
                    code: id.clone(),
                    seats: Default::default(),
                    host: 0,
                    game: None,
                    due: Instant::now(),
                    touched: Instant::now(),
                };
                room.seats[0] = Some(player(name, connection, outbox));
                rooms.insert(id.clone(), room);
                (id, 0)
            }
            ClientMessage::Join { name, code } => {
                let name = clean_name(name)?;
                let code = code.trim().to_uppercase();
                let room = rooms
                    .get_mut(&code)
                    .ok_or("No table has that code. Check the invitation.")?;
                if room.game.is_some() {
                    return Err("This table is already playing.".into());
                }
                let seat = room
                    .seats
                    .iter()
                    .position(Option::is_none)
                    .ok_or("This table is full.")?;
                room.seats[seat] = Some(player(name, connection, outbox));
                (code, seat)
            }
            ClientMessage::Resume { code, token } => {
                let room = rooms
                    .get_mut(&code)
                    .ok_or("Your saved seat has expired. Create or join a new table.")?;
                let seat = room
                    .seats
                    .iter()
                    .position(|p| p.as_ref().is_some_and(|p| !p.bot && p.token == token))
                    .ok_or("Your saved seat has expired. Create or join a new table.")?;
                let p = room.seats[seat].as_mut().unwrap();
                if let Some(previous) = p.outbox.take() {
                    let _ = previous.try_send(ServerMessage::Left);
                }
                p.connection = connection;
                p.outbox = Some(outbox.clone());
                (code, seat)
            }
            _ => unreachable!(),
        };
        let room = rooms.get_mut(&code).unwrap();
        let token = room.seats[seat].as_ref().unwrap().token.clone();
        *session = Some((code.clone(), token.clone()));
        let _ = outbox.try_send(ServerMessage::Session { code, token });
        room.transfer_host();
        room.schedule();
        room.broadcast();
        return Ok(());
    }
    let (id, token) = session.as_ref().ok_or("Join a table first.")?;
    let room = rooms.get_mut(id).ok_or("The table has expired.")?;
    let seat = room
        .seats
        .iter()
        .position(|p| {
            p.as_ref()
                .is_some_and(|p| p.token == *token && p.connection == connection && !p.bot)
        })
        .ok_or("This seat is now connected elsewhere.")?;
    let host = seat == room.host;
    match message {
        ClientMessage::Sit { seat: target } => {
            if room.game.is_some() || target >= 4 || room.seats[target].is_some() {
                return Err("Choose an empty seat before starting.".into());
            }
            room.seats[target] = room.seats[seat].take();
            if host {
                room.host = target;
            }
        }
        ClientMessage::AddBot { seat: target } => {
            if !host || target >= 4 {
                return Err("Only the host can add a bot.".into());
            }
            if room.seats[target]
                .as_ref()
                .is_some_and(|p| p.bot || p.outbox.is_some())
            {
                return Err("That seat is occupied.".into());
            }
            room.seats[target] = Some(Player {
                name: ["Boris", "Mila", "Viktor", "Dara"][target].into(),
                token: String::new(),
                outbox: None,
                connection: 0,
                bot: true,
            });
        }
        ClientMessage::RemoveBot { seat: target } => {
            if !host
                || target >= 4
                || room.game.is_some()
                || !room.seats[target].as_ref().is_some_and(|p| p.bot)
            {
                return Err("Only the host can remove a bot before the game.".into());
            }
            room.seats[target] = None;
        }
        ClientMessage::Start => {
            if !host || room.game.is_some() {
                return Err("Only the host can start a waiting table.".into());
            }
            if !room
                .seats
                .iter()
                .all(|p| p.as_ref().is_some_and(|p| p.view().connected))
            {
                return Err("Fill all four seats and wait for everyone to connect.".into());
            }
            room.game = Some(Game::new(shuffled_deck())?);
        }
        ClientMessage::Bid { bid } => room
            .game
            .as_mut()
            .ok_or("Start the game first.")?
            .bid(seat, bid)?,
        ClientMessage::Play { card, declarations } => room
            .game
            .as_mut()
            .ok_or("Start the game first.")?
            .play(seat, card, &declarations)?,
        ClientMessage::NextRound => {
            if !host {
                return Err("The host starts the next hand.".into());
            }
            let game = room.game.as_mut().ok_or("Start the game first.")?;
            if game.phase == Phase::MatchEnd {
                *game = Game::new(shuffled_deck())?;
            } else {
                game.next_round(shuffled_deck())?;
            }
        }
        _ => return Err("This action is not available.".into()),
    }
    room.schedule();
    room.broadcast();
    Ok(())
}

async fn detach(state: &AppState, session: Session, connection: u64, leave: bool) {
    let Some((id, token)) = session else { return };
    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&id) {
        if let Some(seat) = room.seats.iter().position(|p| {
            p.as_ref()
                .is_some_and(|p| p.token == token && p.connection == connection && !p.bot)
        }) {
            if leave && room.game.is_none() {
                room.seats[seat] = None;
            } else {
                room.seats[seat].as_mut().unwrap().outbox = None;
            }
            room.transfer_host();
            room.touched = Instant::now();
            room.broadcast();
        }
    }
}

async fn maintain(state: AppState) {
    let mut timer = tokio::time::interval(Duration::from_millis(150));
    loop {
        timer.tick().await;
        let mut rooms = state.rooms.lock().await;
        rooms.retain(|_, room| {
            room.seats
                .iter()
                .any(|p| p.as_ref().is_some_and(|p| p.outbox.is_some()))
                || room.touched.elapsed() < Duration::from_secs(1800)
        });
        for room in rooms.values_mut() {
            if room.due > Instant::now()
                || !room
                    .seats
                    .iter()
                    .any(|p| p.as_ref().is_some_and(|p| p.outbox.is_some()))
            {
                continue;
            }
            let Some(game) = room.game.as_mut() else {
                continue;
            };
            let result = if game.phase == Phase::TrickEnd {
                Some(game.collect_trick())
            } else if room.seats[game.turn].as_ref().is_some_and(|p| p.bot) {
                match game.phase {
                    Phase::Bidding => Some(game.bid(game.turn, game.bot_bid())),
                    Phase::Playing => {
                        let (card, ds) = game.bot_play();
                        Some(game.play(game.turn, card, &ds))
                    }
                    _ => None,
                }
            } else {
                None
            };
            if let Some(result) = result {
                if let Err(error) = result {
                    tracing::error!(%error, "Bot action rejected");
                }
                room.schedule();
                room.broadcast();
            }
        }
    }
}
