use crate::prelude::*;
use crate::*;

pub(crate) fn start_sync_events_thread(base_url: String, updates: Sender<NativeSyncUpdate>) {
    thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                let _ = updates.send(NativeSyncUpdate::Error(format!(
                    "failed to create sync event runtime: {err:#}"
                )));
                return;
            }
        };

        loop {
            if updates.send(NativeSyncUpdate::Connecting).is_err() {
                break;
            }

            let message = match rt.block_on(stream_sync_events(&base_url, &updates)) {
                Ok(()) => "sync event stream closed".to_string(),
                Err(err) => err.to_string(),
            };
            if updates.send(NativeSyncUpdate::Error(message)).is_err() {
                break;
            }

            thread::sleep(Duration::from_secs(5));
        }
    });
}

pub(crate) async fn stream_sync_events(
    base_url: &str,
    updates: &Sender<NativeSyncUpdate>,
) -> Result<()> {
    let client = EmailClient::new(base_url)?;
    let mut response = client.open_sync_events().await?;
    updates
        .send(NativeSyncUpdate::Connected)
        .map_err(|_| anyhow!("sync event listener stopped"))?;

    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await? {
        let chunk = String::from_utf8_lossy(&chunk);
        for event in drain_sse_sync_events(&mut buffer, chunk.as_ref()) {
            match event {
                Ok(event) => updates
                    .send(NativeSyncUpdate::Event(event))
                    .map_err(|_| anyhow!("sync event listener stopped"))?,
                Err(err) => updates
                    .send(NativeSyncUpdate::Error(err))
                    .map_err(|_| anyhow!("sync event listener stopped"))?,
            }
        }
    }

    Err(anyhow!("sync event stream closed"))
}

pub(crate) fn drain_sse_sync_events(
    buffer: &mut String,
    chunk: &str,
) -> Vec<Result<SyncEvent, String>> {
    buffer.push_str(chunk);
    let mut events = Vec::new();

    while let Some((index, delimiter_len)) = sse_event_boundary(buffer) {
        let block = buffer[..index].to_string();
        buffer.drain(..index + delimiter_len);
        if let Some(event) = parse_sse_sync_event_block(&block) {
            events.push(event);
        }
    }

    events
}

pub(crate) fn sse_event_boundary(buffer: &str) -> Option<(usize, usize)> {
    ["\r\n\r\n", "\n\n", "\r\r"]
        .iter()
        .filter_map(|delimiter| buffer.find(delimiter).map(|index| (index, delimiter.len())))
        .min_by_key(|(index, _)| *index)
}

pub(crate) fn parse_sse_sync_event_block(block: &str) -> Option<Result<SyncEvent, String>> {
    let block = block.replace("\r\n", "\n").replace('\r', "\n");
    let data = block
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");

    if data.trim().is_empty() {
        return None;
    }

    Some(email_client::parse_sync_event(&data).map_err(|err| err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_boundary_picks_the_earliest_delimiter() {
        assert_eq!(sse_event_boundary("data: a"), None);
        assert_eq!(sse_event_boundary("a\n\nb\r\n\r\n"), Some((1, 2)));
        assert_eq!(sse_event_boundary("a\r\n\r\nb\n\n"), Some((1, 4)));
    }
}
