use crate::bridges::CommBridge;
use crate::conductor::events::{SystemEvent, UserEvent};
use anyhow::Result;
use async_trait::async_trait;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tower_http::cors::CorsLayer;
use tracing::info;

pub struct WebBridge {
    tx_sys: broadcast::Sender<SystemEvent>,
}

struct AppState {
    tx_user: mpsc::Sender<UserEvent>,
    tx_sys: broadcast::Sender<SystemEvent>,
}

impl WebBridge {
    pub fn new() -> (Self, mpsc::Receiver<UserEvent>, mpsc::Sender<UserEvent>) {
        let (tx_user, rx_user) = mpsc::channel(100);
        let (tx_sys, _) = broadcast::channel(100);

        (Self { tx_sys }, rx_user, tx_user)
    }

    pub async fn run_server(self: Arc<Self>, tx_user: mpsc::Sender<UserEvent>) -> Result<()> {
        let state = Arc::new(AppState {
            tx_user,
            tx_sys: self.tx_sys.clone(),
        });

        let app = Router::new()
            .route("/", get(index_html))
            .route("/style.css", get(style_css))
            .route("/app.js", get(app_js))
            .route("/ws", get(ws_handler))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
        info!("Web UI listening on http://127.0.0.1:3000");
        axum::serve(listener, app).await?;

        Ok(())
    }
}

#[async_trait]
impl CommBridge for WebBridge {
    async fn send(&self, event: SystemEvent) -> Result<()> {
        let _ = self.tx_sys.send(event);
        Ok(())
    }
}

async fn index_html() -> Html<&'static str> {
    Html(include_str!("../../web/index.html"))
}

async fn style_css() -> impl IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "text/css")], include_str!("../../web/style.css"))
}

async fn app_js() -> impl IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "application/javascript")], include_str!("../../web/app.js"))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx_sys = state.tx_sys.subscribe();

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx_sys.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    let tx_user = state.tx_user.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let text = text.to_string();
            if text.trim() == "/memory" {
                 let _ = tx_user.send(UserEvent::ToggleMemory).await;
            } else {
                 let _ = tx_user.send(UserEvent::Input(text)).await;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }
}
