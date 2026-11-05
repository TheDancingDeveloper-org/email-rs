//! Undo send: after a send the server holds the message in the outbox until
//! `cancelUntil`; the toast offers `DELETE /outbox/{id}` until then.

use crate::prelude::*;
use crate::*;

/// A just-sent message that can still be cancelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingSend {
    pub(crate) outbox_id: String,
    pub(crate) cancel_until: Instant,
    pub(crate) cancelling: bool,
}

/// Builds the undo window from a send response, or `None` when the server
/// gave no id/deadline (nothing to cancel) or the window has already closed.
pub(crate) fn pending_send_from_response(
    response: &SendResponse,
    now_utc: chrono::DateTime<Utc>,
    now: Instant,
) -> Option<PendingSend> {
    let outbox_id = response.id.clone().filter(|id| !id.is_empty())?;
    let deadline = chrono::DateTime::parse_from_rfc3339(response.cancel_until.as_deref()?)
        .ok()?
        .with_timezone(&Utc);
    let remaining = (deadline - now_utc).to_std().ok()?;
    if remaining.is_zero() {
        return None;
    }
    Some(PendingSend {
        outbox_id,
        cancel_until: now + remaining,
        cancelling: false,
    })
}

/// Drops the toast once its window has closed. Returns whether it changed.
pub(crate) fn expire_pending_send(pending: &mut Option<PendingSend>, now: Instant) -> bool {
    if pending
        .as_ref()
        .is_some_and(|send| !send.cancelling && now >= send.cancel_until)
    {
        *pending = None;
        return true;
    }
    false
}

pub(crate) fn dismiss_pending_send(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.pending_send = None;
        cx.notify();
    });
}

/// Cancels the queued send. Marks the toast busy and returns the outbox id to
/// cancel, or `None` when there is nothing cancellable.
pub(crate) fn begin_cancel_send(state: &mut NativeAppState, now: Instant) -> Option<String> {
    expire_pending_send(&mut state.pending_send, now);
    let send = state
        .pending_send
        .as_mut()
        .filter(|send| !send.cancelling)?;
    send.cancelling = true;
    Some(send.outbox_id.clone())
}

pub(crate) fn finish_cancel_send(
    state: &mut NativeAppState,
    result: std::result::Result<(), String>,
) {
    state.pending_send = None;
    match result {
        Ok(()) => {
            state.compose.error = None;
            state.compose.notice = Some("Send cancelled".to_string());
            state.last_error = None;
        }
        Err(error) => {
            let message = format!("Could not cancel the send: {error}");
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
        }
    }
}

pub(crate) fn start_cancel_send(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let Some((base_url, outbox_id)) = state_entity.update(app_cx, |state, cx| {
        let id = begin_cancel_send(state, Instant::now())?;
        cx.notify();
        Some((state.api_base_url.clone(), id))
    }) else {
        return;
    };

    let task = app_cx
        .background_executor()
        .spawn(async move { cancel_outbox_request(base_url, outbox_id) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await.map_err(|err| format!("{err:#}"));
                state_entity.update(&mut async_cx, |state, cx| {
                    finish_cancel_send(state, result);
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn cancel_outbox_request(base_url: String, outbox_id: String) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client.cancel_outbox(&outbox_id).await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(id: Option<&str>, cancel_until: Option<&str>) -> SendResponse {
        SendResponse {
            status: "queued".into(),
            id: id.map(str::to_string),
            scheduled_at: None,
            cancel_until: cancel_until.map(str::to_string),
        }
    }

    fn at(s: &str) -> chrono::DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn window_follows_the_server_deadline() {
        let now = Instant::now();
        let send = pending_send_from_response(
            &response(Some("o1"), Some("2026-01-01T00:00:10Z")),
            at("2026-01-01T00:00:00Z"),
            now,
        )
        .unwrap();
        assert_eq!(send.outbox_id, "o1");
        assert_eq!(send.cancel_until, now + Duration::from_secs(10));
        assert!(!send.cancelling);
    }

    #[test]
    fn no_toast_without_id_deadline_or_open_window() {
        let now = Instant::now();
        let t0 = at("2026-01-01T00:00:00Z");
        assert!(
            pending_send_from_response(&response(None, Some("2026-01-01T00:00:10Z")), t0, now)
                .is_none()
        );
        assert!(pending_send_from_response(&response(Some("o1"), None), t0, now).is_none());
        assert!(
            pending_send_from_response(&response(Some("o1"), Some("garbage")), t0, now).is_none()
        );
        assert!(pending_send_from_response(
            &response(Some("o1"), Some("2026-01-01T00:00:00Z")),
            t0,
            now
        )
        .is_none());
        assert!(pending_send_from_response(
            &response(Some("o1"), Some("2025-12-31T23:59:00Z")),
            t0,
            now
        )
        .is_none());
    }

    fn state_with(send: PendingSend) -> NativeAppState {
        NativeAppState {
            pending_send: Some(send),
            ..NativeAppState::default()
        }
    }

    fn open_send(now: Instant) -> PendingSend {
        PendingSend {
            outbox_id: "o1".into(),
            cancel_until: now + Duration::from_secs(5),
            cancelling: false,
        }
    }

    #[test]
    fn toast_expires_at_the_deadline_not_before() {
        let now = Instant::now();
        let mut pending = Some(open_send(now));
        assert!(!expire_pending_send(
            &mut pending,
            now + Duration::from_secs(4)
        ));
        assert!(pending.is_some());
        assert!(expire_pending_send(
            &mut pending,
            now + Duration::from_secs(5)
        ));
        assert!(pending.is_none());
    }

    #[test]
    fn cancel_is_requested_once_and_never_after_the_window() {
        let now = Instant::now();
        let mut state = state_with(open_send(now));
        assert_eq!(begin_cancel_send(&mut state, now), Some("o1".to_string()));
        assert!(begin_cancel_send(&mut state, now).is_none(), "double click");

        let mut late = state_with(open_send(now));
        assert!(begin_cancel_send(&mut late, now + Duration::from_secs(6)).is_none());
        assert!(late.pending_send.is_none());

        let mut none = NativeAppState::default();
        assert!(begin_cancel_send(&mut none, now).is_none());
    }

    #[test]
    fn cancel_success_clears_toast_and_says_so() {
        let mut state = state_with(open_send(Instant::now()));
        finish_cancel_send(&mut state, Ok(()));
        assert!(state.pending_send.is_none());
        assert_eq!(state.compose.notice.as_deref(), Some("Send cancelled"));
        assert!(state.compose.error.is_none());
    }

    #[test]
    fn cancel_failure_such_as_409_is_surfaced() {
        let mut state = state_with(open_send(Instant::now()));
        finish_cancel_send(&mut state, Err("409 Conflict".into()));
        assert!(state.pending_send.is_none());
        assert!(state.compose.error.as_deref().unwrap().contains("409"));
        assert!(state.last_error.is_some());
    }

    #[test]
    fn cancel_outbox_request_hits_the_delete_route() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = r#"{"id":"o1","status":"cancelled"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            request
        });
        cancel_outbox_request(format!("http://{addr}"), "o1".into()).unwrap();
        assert!(server
            .join()
            .unwrap()
            .starts_with("DELETE /api/v1/outbox/o1 "));
    }
}
