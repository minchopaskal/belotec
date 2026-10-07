use belote_core::*;
use futures_util::{SinkExt, StreamExt};
use std::{collections::HashSet, path::PathBuf, time::Duration};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
struct Client {
    socket: Socket,
    room: Option<RoomView>,
    token: String,
}
impl Client {
    async fn connect(url: &str) -> Self {
        Self {
            socket: connect_async(url).await.unwrap().0,
            room: None,
            token: String::new(),
        }
    }
    async fn send(&mut self, m: ClientMessage) {
        self.socket
            .send(Message::Text(serde_json::to_string(&m).unwrap().into()))
            .await
            .unwrap();
    }
    async fn receive(&mut self) -> ServerMessage {
        loop {
            let m = tokio::time::timeout(Duration::from_secs(5), self.socket.next())
                .await
                .expect("server response timeout")
                .unwrap()
                .unwrap();
            if let Message::Text(s) = m {
                let m: ServerMessage = serde_json::from_str(&s).unwrap();
                match &m {
                    ServerMessage::Session { token, .. } => self.token = token.clone(),
                    ServerMessage::State { room } => self.room = Some(*room.clone()),
                    _ => {}
                }
                return m;
            }
        }
    }
    async fn state(&mut self) {
        loop {
            match self.receive().await {
                ServerMessage::State { .. } => return,
                ServerMessage::Error { message } => panic!("unexpected server error: {message}"),
                _ => {}
            }
        }
    }
    async fn error(&mut self) -> String {
        loop {
            if let ServerMessage::Error { message } = self.receive().await {
                return message;
            }
        }
    }
}

async fn server() -> (
    String,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    let (app, maintenance) =
        belote_server::app(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let serving = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("ws://{address}/ws"), serving, maintenance)
}
async fn synchronize(clients: &mut [Client]) {
    for client in clients {
        client.state().await;
    }
}

#[tokio::test]
async fn four_clients_play_a_hand_reconnect_and_keep_private_hands() {
    let (url, serving, maintenance) = server().await;
    let mut clients = Vec::new();
    let mut host = Client::connect(&url).await;
    host.send(ClientMessage::Create {
        name: "Alice".into(),
    })
    .await;
    host.state().await;
    let code = host.room.as_ref().unwrap().code.clone();
    clients.push(host);
    for name in ["Boris", "Mila", "Dara"] {
        let mut friend = Client::connect(&url).await;
        friend
            .send(ClientMessage::Join {
                name: name.into(),
                code: code.clone(),
            })
            .await;
        friend.state().await;
        synchronize(&mut clients).await;
        clients.push(friend);
    }
    let mut stranger = Client::connect(&url).await;
    stranger
        .send(ClientMessage::Join {
            name: "Fifth".into(),
            code: code.clone(),
        })
        .await;
    assert!(stranger.error().await.contains("full"));
    clients[1].send(ClientMessage::Start).await;
    assert!(clients[1].error().await.contains("host"));
    clients[0].send(ClientMessage::Start).await;
    synchronize(&mut clients).await;
    let mut cards = HashSet::new();
    for c in &clients {
        let g = c.room.as_ref().unwrap().game.as_ref().unwrap();
        assert_eq!(g.hand.len(), 5);
        for card in &g.hand {
            assert!(cards.insert(*card));
        }
    }
    clients[1]
        .send(ClientMessage::Bid {
            bid: Bid::Contract(Contract::AllTrumps),
        })
        .await;
    assert!(clients[1].error().await.contains("turn"));
    clients[0]
        .send(ClientMessage::Bid {
            bid: Bid::Contract(Contract::AllTrumps),
        })
        .await;
    synchronize(&mut clients).await;
    for s in [1, 2, 3] {
        clients[s].send(ClientMessage::Bid { bid: Bid::Pass }).await;
        synchronize(&mut clients).await;
    }
    cards.clear();
    for c in &clients {
        for card in &c.room.as_ref().unwrap().game.as_ref().unwrap().hand {
            assert!(cards.insert(*card));
        }
    }
    assert_eq!(cards.len(), 32);
    // Reconnect one client without losing its hand or seat.
    let before = clients[2]
        .room
        .as_ref()
        .unwrap()
        .game
        .as_ref()
        .unwrap()
        .hand
        .clone();
    let token = clients[2].token.clone();
    clients[2].socket.close(None).await.unwrap();
    for s in [0, 1, 3] {
        clients[s].state().await;
        assert!(
            !clients[s].room.as_ref().unwrap().seats[2]
                .as_ref()
                .unwrap()
                .connected
        );
    }
    let mut resumed = Client::connect(&url).await;
    resumed
        .send(ClientMessage::Resume {
            code: code.clone(),
            token,
        })
        .await;
    resumed.state().await;
    assert_eq!(resumed.room.as_ref().unwrap().you, 2);
    assert_eq!(
        resumed.room.as_ref().unwrap().game.as_ref().unwrap().hand,
        before
    );
    for s in [0, 1, 3] {
        clients[s].state().await;
    }
    clients[2] = resumed;
    let foreign_card = clients[1]
        .room
        .as_ref()
        .unwrap()
        .game
        .as_ref()
        .unwrap()
        .hand[0];
    clients[0]
        .send(ClientMessage::Play {
            card: foreign_card,
            declarations: vec![],
        })
        .await;
    assert!(clients[0].error().await.contains("highlighted"));
    for _ in 0..8 {
        for _ in 0..4 {
            let turn = clients[0]
                .room
                .as_ref()
                .unwrap()
                .game
                .as_ref()
                .unwrap()
                .turn;
            let g = clients[turn].room.as_ref().unwrap().game.as_ref().unwrap();
            let card = g.legal_cards[0];
            let declarations = best_declarations(&g.declarations);
            clients[turn]
                .send(ClientMessage::Play { card, declarations })
                .await;
            synchronize(&mut clients).await;
        }
        assert_eq!(
            clients[0]
                .room
                .as_ref()
                .unwrap()
                .game
                .as_ref()
                .unwrap()
                .phase,
            Phase::TrickEnd
        );
        synchronize(&mut clients).await;
    }
    let game = clients[0].room.as_ref().unwrap().game.as_ref().unwrap();
    assert_eq!(game.phase, Phase::RoundEnd);
    assert_eq!(game.history.len(), 1);
    assert_eq!(game.tricks.iter().sum::<u8>(), 8);
    for c in &clients {
        let other = c.room.as_ref().unwrap().game.as_ref().unwrap();
        assert_eq!(other.scores, game.scores);
        assert_eq!(other.history, game.history);
        assert!(other.hand.is_empty());
    }
    clients[0].send(ClientMessage::NextRound).await;
    synchronize(&mut clients).await;
    assert_eq!(
        clients[0]
            .room
            .as_ref()
            .unwrap()
            .game
            .as_ref()
            .unwrap()
            .dealer,
        0
    );
    serving.abort();
    maintenance.abort();
}

#[tokio::test]
async fn invalid_sessions_names_origins_and_host_transfer() {
    let (url, serving, maintenance) = server().await;
    let mut request = url.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", "https://example.com".parse().unwrap());
    assert!(connect_async(request).await.is_err());
    let mut a = Client::connect(&url).await;
    a.send(ClientMessage::Create { name: "\n".into() }).await;
    assert!(a.error().await.contains("name"));
    a.send(ClientMessage::Create {
        name: "Host".into(),
    })
    .await;
    a.state().await;
    let code = a.room.as_ref().unwrap().code.clone();
    let mut b = Client::connect(&url).await;
    b.send(ClientMessage::Resume {
        code: code.clone(),
        token: "wrong".into(),
    })
    .await;
    assert!(b.error().await.contains("expired"));
    b.send(ClientMessage::Join {
        name: "Guest".into(),
        code: code.clone(),
    })
    .await;
    b.state().await;
    a.state().await;
    b.send(ClientMessage::AddBot { seat: 2 }).await;
    assert!(b.error().await.contains("host"));
    a.send(ClientMessage::Sit { seat: usize::MAX }).await;
    assert!(a.error().await.contains("empty"));
    a.send(ClientMessage::Leave).await;
    while !matches!(a.receive().await, ServerMessage::Left) {}
    b.state().await;
    assert_eq!(b.room.as_ref().unwrap().host, 1);
    b.send(ClientMessage::AddBot { seat: 0 }).await;
    b.state().await;
    assert!(b.room.as_ref().unwrap().seats[0].as_ref().unwrap().bot);
    b.send(ClientMessage::RemoveBot { seat: 0 }).await;
    b.state().await;
    assert!(b.room.as_ref().unwrap().seats[0].is_none());
    serving.abort();
    maintenance.abort();
}
