use anyhow::Result;
use axum::{
    extract::{Query, State},
    routing::get,
    Router,
};
use rspotify::{
    prelude::*,
    scopes,
    AuthCodePkceSpotify, Credentials, OAuth,
};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::{Mutex, oneshot};

#[derive(Clone)]
struct AppState {
    spotify: Arc<Mutex<AuthCodePkceSpotify>>,
    sender: Arc<Mutex<Option<oneshot::Sender<Tokens>>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
}

pub async fn authenticate(
    client_id: String,
    client_secret: String,
) -> Result<Tokens> {
    let creds = Credentials::new(&client_id, &client_secret);
    let oauth = OAuth {
        redirect_uri: "http://127.0.0.1:8888/callback".to_string(),
        scopes: scopes!(
            "user-read-private",
            "user-read-email",
            "playlist-read-private",
            "playlist-read-collaborative",
            "user-library-read",
            "user-modify-playback-state",
            "user-read-currently-playing",
            "user-read-playback-state"
        ),
        ..Default::default()
    };

    let mut spotify = AuthCodePkceSpotify::new(creds, oauth);

    let url = spotify.get_authorize_url(None)?;
    webbrowser::open(&url)?;

    let (tx, rx) = oneshot::channel();

    let state = AppState {
        spotify: Arc::new(Mutex::new(spotify)),
        sender: Arc::new(Mutex::new(Some(tx))),
    };

    let app = Router::new()
        .route("/callback", get(handle_callback))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8888));
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        shutdown_rx.await.ok();
    });

    tokio::spawn(async move { server.await.unwrap() });

    let tokens = rx.await?;
    shutdown_tx.send(()).ok();
    Ok(tokens)
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
}

async fn handle_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> &'static str {
    let spotify = state.spotify.lock().await;
    if let Err(_) = spotify.request_token(&params.code).await {
        return "Failed to get token";
    }

    if let Some(token) = spotify.get_token().lock().await.unwrap().as_ref() {
        let tokens = Tokens {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.as_ref().unwrap().clone(),
        };
        if let Some(sender) = state.sender.lock().await.take() {
            sender.send(tokens).ok();
        }
    }

    "Authentication successful! You can close this window."
}
