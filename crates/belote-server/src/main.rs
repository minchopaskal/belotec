use std::{net::SocketAddr, path::PathBuf};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "belote_server=info,tower_http=info".into()),
        )
        .init();
    let address: SocketAddr = std::env::var("BELOTE_BIND")
        .unwrap_or_else(|_| "0.0.0.0:3000".into())
        .parse()
        .expect("BELOTE_BIND must be an IP:port");
    let web = std::env::var_os("BELOTE_WEB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web"));
    if !web.join("pkg/belote_client_bg.wasm").exists() {
        tracing::warn!("Browser client is not built yet. Run ./scripts/build.sh first.");
    }
    let (app, maintenance) = belote_server::app(web);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("Could not bind the server port");
    tracing::info!(
        "Belote is ready at http://localhost:{} — friends can use this computer's LAN IP",
        address.port()
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
    maintenance.abort();
}
