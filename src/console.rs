use crate::{
    at_policy::CommandClass,
    server::{self, App},
};
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{sync::Arc, time::Instant};
use tokio::time::{Duration, timeout};

pub async fn websocket(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !server::origin_allowed(&headers, !app.config.no_tls) {
        return server::json_response(
            403,
            json!({"ok": false, "error": "websocket origin forbidden"}),
        );
    }
    let token = server::cookie(&headers);
    let Some(_) = app.auth.watch(&token) else {
        return server::json_response(401, json!({"ok": false, "error": "login required"}));
    };
    let permit = match app.sockets.clone().try_acquire_owned() {
        Ok(value) => value,
        Err(_) => {
            return server::json_response(
                503,
                json!({"ok": false, "error": "too many connections"}),
            );
        }
    };
    upgrade
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            run(app, socket, token).await;
        })
        .into_response()
}

async fn send(socket: &mut WebSocket, value: serde_json::Value) -> bool {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .is_ok()
}

async fn run(app: Arc<App>, mut socket: WebSocket, token: String) {
    let mut last_command = None;
    while let Some(message) = socket.recv().await {
        if !app.auth.valid(&token, false) {
            break;
        }
        let Message::Text(text) = (match message {
            Ok(value) => value,
            Err(_) => break,
        }) else {
            continue;
        };
        let text = text.trim();
        let (confirmed, command) = match text.strip_prefix("CONFIRM ") {
            Some(command) => (true, command.trim()),
            None => (false, text),
        };
        if command.is_empty() || command.len() > 4096 || command.chars().any(char::is_control) {
            if !send(
                &mut socket,
                json!({"ok": false, "error": "invalid AT command"}),
            )
            .await
            {
                break;
            }
            continue;
        }
        let now = Instant::now();
        if last_command.is_some_and(|last| now.duration_since(last) < Duration::from_secs(1)) {
            if !send(
                &mut socket,
                json!({"ok": false, "error": "AT command rate limited"}),
            )
            .await
            {
                break;
            }
            continue;
        }
        last_command = Some(now);
        if let Err(error) = crate::at_policy::validate(command) {
            server::audit_command(&app, command, "rejected-retired-command");
            if !send(
                &mut socket,
                json!({"ok": false, "error": error.to_string()}),
            )
            .await
            {
                break;
            }
            continue;
        }
        let class = crate::at_policy::classify(command).unwrap_or(CommandClass::SensitiveWrite);
        if !matches!(class, CommandClass::ReadOnly) && !confirmed {
            if !send(&mut socket, json!({"ok": false, "error": "explicit confirmation required; resend as CONFIRM <AT command>"})).await { break; }
            continue;
        }
        let response = timeout(Duration::from_secs(120), app.at.run(command)).await;
        let reply = match response {
            Ok(Ok(raw)) => {
                if !matches!(class, CommandClass::ReadOnly) {
                    app.at.invalidate().await;
                }
                server::audit_command(
                    &app,
                    command,
                    if crate::parser::ok(&raw) {
                        "executed"
                    } else {
                        "modem-error"
                    },
                );
                json!({"ok": crate::parser::ok(&raw), "response": raw})
            }
            Ok(Err(error)) => {
                server::audit_command(&app, command, "error");
                json!({"ok": false, "error": error.to_string()})
            }
            Err(_) => {
                server::audit_command(&app, command, "timeout");
                json!({"ok": false, "error": "AT command timed out"})
            }
        };
        if !send(&mut socket, reply).await {
            break;
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

pub fn page() -> &'static str {
    include_str!("console.html")
}
