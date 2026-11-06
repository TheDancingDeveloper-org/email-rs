//! Moving one message between IMAP folders without touching any other message.
//!
//! With `MOVE` (RFC 6851) the server does the whole job. Without it the move
//! is `UID COPY`, `UID STORE +FLAGS (\Deleted)` and `UID EXPUNGE <uid>`
//! (RFC 4315, `UIDPLUS`). A plain `EXPUNGE` removes every `\Deleted` message
//! in the folder, including ones the user flagged for another reason, so a
//! server without `UIDPLUS` gets a refusal instead of that fallback.
//!
//! The destination UID comes from the `COPYUID` response code (RFC 4315),
//! which a server sends in the tagged `OK` of `UID COPY` or in an untagged
//! `OK` during `UID MOVE`.

use std::fmt;

use async_imap::imap_proto::{RequestId, Response, ResponseCode, Status, UidSetMember};
use async_imap::Session;
use tokio::io::{AsyncRead, AsyncWrite};

use super::MovedMessage;
use crate::error::{AppError, Result};

pub(crate) async fn copy_uid<T>(
    session: &mut Session<T>,
    src_folder: &str,
    uid: u32,
    dest_folder: &str,
) -> Result<MovedMessage>
where
    T: AsyncRead + AsyncWrite + Unpin + fmt::Debug + Send,
{
    let src = quote_mailbox(src_folder)?;
    let dest = quote_mailbox(dest_folder)?;
    run_tagged(session, "SELECT", &format!("SELECT {src}")).await?;
    let copied = run_tagged(session, "UID COPY", &format!("UID COPY {uid} {dest}")).await?;
    Ok(moved_from(copied, uid))
}

/// Move `uid` from `src_folder` to `dest_folder` on an authenticated session.
pub(crate) async fn move_uid<T>(
    session: &mut Session<T>,
    src_folder: &str,
    uid: u32,
    dest_folder: &str,
) -> Result<MovedMessage>
where
    T: AsyncRead + AsyncWrite + Unpin + fmt::Debug + Send,
{
    let src = quote_mailbox(src_folder)?;
    let dest = quote_mailbox(dest_folder)?;
    run_tagged(session, "SELECT", &format!("SELECT {src}")).await?;
    let capabilities = session
        .capabilities()
        .await
        .map_err(|e| AppError::Imap(format!("CAPABILITY failed: {e}")))?;
    let has_move = capabilities.has_str("MOVE");
    let has_uidplus = capabilities.has_str("UIDPLUS");

    if has_move {
        let copied = run_tagged(session, "UID MOVE", &format!("UID MOVE {uid} {dest}")).await?;
        return Ok(moved_from(copied, uid));
    }
    if !has_uidplus {
        return Err(AppError::Provider(format!(
            "this server supports neither MOVE nor UIDPLUS, so message {uid} cannot be moved out \
             of {src_folder} without also expunging other deleted messages; nothing was changed"
        )));
    }
    // COPY first. If it fails nothing is flagged or expunged.
    let copied = run_tagged(session, "UID COPY", &format!("UID COPY {uid} {dest}")).await?;
    run_tagged(
        session,
        "UID STORE \\Deleted",
        &format!("UID STORE {uid} +FLAGS (\\Deleted)"),
    )
    .await?;
    run_tagged(session, "UID EXPUNGE", &format!("UID EXPUNGE {uid}")).await?;
    Ok(moved_from(copied, uid))
}

/// Delete `uid` from `folder` without touching any other `\Deleted` message.
///
/// Refuses before changing anything when the server lacks `UIDPLUS`, because a
/// plain `EXPUNGE` would remove every message flagged `\Deleted` in the
/// folder. With `copy_to` the message is first copied there (for example to
/// Trash); a failed copy flags and expunges nothing.
pub(crate) async fn delete_uid<T>(
    session: &mut Session<T>,
    folder: &str,
    uid: u32,
    copy_to: Option<&str>,
) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + fmt::Debug + Send,
{
    let src = quote_mailbox(folder)?;
    let dest = copy_to.map(quote_mailbox).transpose()?;
    run_tagged(session, "SELECT", &format!("SELECT {src}")).await?;
    let capabilities = session
        .capabilities()
        .await
        .map_err(|e| AppError::Imap(format!("CAPABILITY failed: {e}")))?;
    if !capabilities.has_str("UIDPLUS") {
        return Err(AppError::Provider(format!(
            "this server does not support UIDPLUS, so message {uid} cannot be deleted from \
             {folder} without also expunging other deleted messages; nothing was changed"
        )));
    }
    if let Some(dest) = dest {
        run_tagged(session, "UID COPY", &format!("UID COPY {uid} {dest}")).await?;
    }
    run_tagged(
        session,
        "UID STORE \\Deleted",
        &format!("UID STORE {uid} +FLAGS (\\Deleted)"),
    )
    .await?;
    run_tagged(session, "UID EXPUNGE", &format!("UID EXPUNGE {uid}")).await?;
    Ok(())
}

fn moved_from(copied: Option<CopyUid>, uid: u32) -> MovedMessage {
    let dest_uid = copied.as_ref().and_then(|c| {
        c.source
            .iter()
            .position(|source| *source == uid)
            .and_then(|index| c.destination.get(index).copied())
    });
    MovedMessage {
        uid: dest_uid,
        uid_validity: dest_uid.and(copied.map(|c| c.uid_validity)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CopyUid {
    uid_validity: u32,
    source: Vec<u32>,
    destination: Vec<u32>,
}

/// Longest UID range expanded from a `COPYUID` set; a single-message move
/// never needs more than one UID.
const MAX_COPYUID_UIDS: usize = 1024;

fn expand_uid_set(set: &[UidSetMember]) -> Vec<u32> {
    let mut uids = Vec::new();
    for member in set {
        match member {
            UidSetMember::Uid(uid) => uids.push(*uid),
            UidSetMember::UidRange(range) => uids.extend(
                range
                    .clone()
                    .take(MAX_COPYUID_UIDS.saturating_sub(uids.len())),
            ),
        }
        if uids.len() >= MAX_COPYUID_UIDS {
            break;
        }
    }
    uids
}

fn copy_uid_from(code: Option<&ResponseCode<'_>>) -> Option<CopyUid> {
    match code? {
        ResponseCode::CopyUid(uid_validity, source, destination) => Some(CopyUid {
            uid_validity: *uid_validity,
            source: expand_uid_set(source),
            destination: expand_uid_set(destination),
        }),
        _ => None,
    }
}

/// Send one tagged command and read until its completion, returning any
/// `COPYUID` seen. A `NO` or `BAD` completion is an error.
async fn run_tagged<T>(
    session: &mut Session<T>,
    what: &str,
    command: &str,
) -> Result<Option<CopyUid>>
where
    T: AsyncRead + AsyncWrite + Unpin + fmt::Debug + Send,
{
    let tag: RequestId = session
        .run_command(command)
        .await
        .map_err(|e| AppError::Imap(format!("{what} failed: {e}")))?;
    let mut copy_uid = None;
    loop {
        let response = session
            .read_response()
            .await
            .ok_or_else(|| AppError::Imap(format!("connection lost during {what}")))?
            .map_err(|e| AppError::Imap(format!("{what} failed: {e}")))?;
        match response.parsed() {
            Response::Data {
                status: Status::Ok,
                code,
                ..
            } => {
                if let Some(found) = copy_uid_from(code.as_ref()) {
                    copy_uid = Some(found);
                }
            }
            Response::Done {
                tag: done_tag,
                status,
                code,
                information,
            } if *done_tag == tag => {
                if *status != Status::Ok {
                    let detail: String = information
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(200)
                        .collect();
                    return Err(AppError::Imap(format!(
                        "{what} failed: {status:?} {detail}"
                    )));
                }
                if let Some(found) = copy_uid_from(code.as_ref()) {
                    copy_uid = Some(found);
                }
                return Ok(copy_uid);
            }
            _ => {}
        }
    }
}

/// An IMAP quoted string for a mailbox name. Names the quoted form cannot
/// carry are rejected rather than sent as a different command.
fn quote_mailbox(name: &str) -> Result<String> {
    if name.is_empty() || name.chars().any(|c| c.is_control()) {
        return Err(AppError::Validation(format!(
            "folder name {name:?} cannot be sent to an IMAP server"
        )));
    }
    Ok(format!(
        "\"{}\"",
        name.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::*;

    /// How the scripted server answers `UID COPY`.
    #[derive(Clone, Copy)]
    enum CopyReply {
        Ok,
        No,
    }

    /// A scripted IMAP server on an in-memory stream. It records every command
    /// after the tag and answers just enough of the protocol for a move.
    async fn session_with(
        capabilities: &'static str,
        copy: CopyReply,
    ) -> (Session<tokio::io::DuplexStream>, Arc<Mutex<Vec<String>>>) {
        let (client_end, server_end) = tokio::io::duplex(8192);
        let log = Arc::new(Mutex::new(Vec::new()));
        let server_log = Arc::clone(&log);
        tokio::spawn(async move {
            let (read, mut write) = tokio::io::split(server_end);
            let mut lines = BufReader::new(read).lines();
            write.write_all(b"* OK scripted ready\r\n").await.unwrap();
            while let Ok(Some(line)) = lines.next_line().await {
                let (tag, command) = line.split_once(' ').unwrap();
                let reply = match command.split(' ').next().unwrap() {
                    "LOGIN" => format!("{tag} OK logged in\r\n"),
                    "SELECT" => {
                        server_log.lock().unwrap().push(command.to_string());
                        format!("* 3 EXISTS\r\n* OK [UIDVALIDITY 5] ok\r\n{tag} OK [READ-WRITE] done\r\n")
                    }
                    "CAPABILITY" => {
                        server_log.lock().unwrap().push(command.to_string());
                        format!("* CAPABILITY IMAP4rev1 {capabilities}\r\n{tag} OK done\r\n")
                    }
                    _ => {
                        server_log.lock().unwrap().push(command.to_string());
                        if command.starts_with("UID MOVE") {
                            format!(
                                "* OK [COPYUID 42 7 99] moved\r\n* 1 EXPUNGE\r\n{tag} OK done\r\n"
                            )
                        } else if command.starts_with("UID COPY") {
                            match copy {
                                CopyReply::Ok => {
                                    format!("{tag} OK [COPYUID 42 7 99] copied\r\n")
                                }
                                CopyReply::No => {
                                    format!("{tag} NO [TRYCREATE] no such mailbox\r\n")
                                }
                            }
                        } else if command.starts_with("UID STORE") {
                            format!("* 1 FETCH (UID 7 FLAGS (\\Deleted))\r\n{tag} OK done\r\n")
                        } else if command.starts_with("UID EXPUNGE") || command == "EXPUNGE" {
                            format!("* 1 EXPUNGE\r\n{tag} OK done\r\n")
                        } else {
                            format!("{tag} BAD unexpected\r\n")
                        }
                    }
                };
                write.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let mut client = async_imap::Client::new(client_end);
        client.read_response().await.unwrap().unwrap();
        let session = client
            .login("user", "pass")
            .await
            .map_err(|(e, _)| e)
            .unwrap();
        (session, log)
    }

    fn commands(log: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        log.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn copy_never_marks_deleted_or_expunges() {
        let (mut session, log) = session_with("", CopyReply::Ok).await;
        let copied = copy_uid(&mut session, "INBOX", 7, "Projects")
            .await
            .unwrap();
        assert_eq!(
            commands(&log),
            ["SELECT \"INBOX\"", "UID COPY 7 \"Projects\""]
        );
        assert_eq!(copied.uid, Some(99));
        assert_eq!(copied.uid_validity, Some(42));
    }

    #[tokio::test]
    async fn failed_copy_never_changes_source_flags() {
        let (mut session, log) = session_with("UIDPLUS MOVE", CopyReply::No).await;
        assert!(copy_uid(&mut session, "INBOX", 7, "Projects")
            .await
            .is_err());
        assert_eq!(
            commands(&log),
            ["SELECT \"INBOX\"", "UID COPY 7 \"Projects\""]
        );
    }

    #[tokio::test]
    async fn move_capability_uses_uid_move_and_reads_the_destination_uid() {
        let (mut session, log) = session_with("UIDPLUS MOVE", CopyReply::Ok).await;
        let moved = move_uid(&mut session, "INBOX", 7, "Archive").await.unwrap();
        assert_eq!(
            commands(&log),
            ["SELECT \"INBOX\"", "CAPABILITY", "UID MOVE 7 \"Archive\""]
        );
        assert_eq!(
            moved,
            MovedMessage {
                uid: Some(99),
                uid_validity: Some(42)
            }
        );
    }

    #[tokio::test]
    async fn copy_path_expunges_only_the_moved_uid() {
        let (mut session, log) = session_with("UIDPLUS", CopyReply::Ok).await;
        let moved = move_uid(&mut session, "INBOX", 7, "My Folder")
            .await
            .unwrap();
        assert_eq!(
            commands(&log),
            [
                "SELECT \"INBOX\"",
                "CAPABILITY",
                "UID COPY 7 \"My Folder\"",
                "UID STORE 7 +FLAGS (\\Deleted)",
                "UID EXPUNGE 7",
            ]
        );
        assert_eq!(
            moved,
            MovedMessage {
                uid: Some(99),
                uid_validity: Some(42)
            }
        );
    }

    #[tokio::test]
    async fn without_uidplus_the_move_is_refused_before_anything_changes() {
        let (mut session, log) = session_with("", CopyReply::Ok).await;
        let error = move_uid(&mut session, "INBOX", 7, "Archive")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("UIDPLUS"), "{error}");
        assert_eq!(commands(&log), ["SELECT \"INBOX\"", "CAPABILITY"]);
    }

    #[tokio::test]
    async fn failed_copy_flags_and_expunges_nothing() {
        let (mut session, log) = session_with("UIDPLUS", CopyReply::No).await;
        move_uid(&mut session, "INBOX", 7, "Missing")
            .await
            .unwrap_err();
        assert_eq!(
            commands(&log),
            ["SELECT \"INBOX\"", "CAPABILITY", "UID COPY 7 \"Missing\""]
        );
    }

    #[tokio::test]
    async fn delete_uid_expunges_only_that_uid() {
        let (mut session, log) = session_with("UIDPLUS", CopyReply::Ok).await;
        delete_uid(&mut session, "INBOX", 7, None).await.unwrap();
        assert_eq!(
            commands(&log),
            [
                "SELECT \"INBOX\"",
                "CAPABILITY",
                "UID STORE 7 +FLAGS (\\Deleted)",
                "UID EXPUNGE 7",
            ]
        );
    }

    #[tokio::test]
    async fn delete_uid_with_trash_copies_first_then_expunges_only_that_uid() {
        let (mut session, log) = session_with("UIDPLUS", CopyReply::Ok).await;
        delete_uid(&mut session, "INBOX", 7, Some("[Gmail]/Trash"))
            .await
            .unwrap();
        assert_eq!(
            commands(&log),
            [
                "SELECT \"INBOX\"",
                "CAPABILITY",
                "UID COPY 7 \"[Gmail]/Trash\"",
                "UID STORE 7 +FLAGS (\\Deleted)",
                "UID EXPUNGE 7",
            ]
        );
    }

    #[tokio::test]
    async fn delete_uid_without_uidplus_is_refused_before_anything_changes() {
        let (mut session, log) = session_with("", CopyReply::Ok).await;
        let error = delete_uid(&mut session, "INBOX", 7, Some("[Gmail]/Trash"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("UIDPLUS"), "{error}");
        assert_eq!(commands(&log), ["SELECT \"INBOX\"", "CAPABILITY"]);
    }

    #[tokio::test]
    async fn delete_uid_failed_copy_flags_and_expunges_nothing() {
        let (mut session, log) = session_with("UIDPLUS", CopyReply::No).await;
        delete_uid(&mut session, "INBOX", 7, Some("[Gmail]/Trash"))
            .await
            .unwrap_err();
        assert_eq!(
            commands(&log),
            [
                "SELECT \"INBOX\"",
                "CAPABILITY",
                "UID COPY 7 \"[Gmail]/Trash\""
            ]
        );
    }

    #[test]
    fn mailbox_names_are_quoted_and_control_characters_rejected() {
        assert_eq!(quote_mailbox("a\"b\\c").unwrap(), "\"a\\\"b\\\\c\"");
        assert!(quote_mailbox("a\r\nb").is_err());
        assert!(quote_mailbox("").is_err());
    }
}
