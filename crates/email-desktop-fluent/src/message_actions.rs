use crate::prelude::*;
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeMessageSource {
    Folder(String),
    Label(String),
    SmartFolder(String),
    Search(String),
    AdvancedSearch(ConditionGroup),
}

impl NativeMessageSource {
    pub(crate) fn from_state(state: &NativeAppState) -> Option<Self> {
        if let Some(group) = state.search.condition_group.as_ref() {
            return Some(Self::AdvancedSearch(group.clone()));
        }

        if let Some(query) = state.search.submitted_query.as_ref() {
            return Some(Self::Search(query.clone()));
        }

        Self::route_from_state(state)
    }

    pub(crate) fn route_from_state(state: &NativeAppState) -> Option<Self> {
        if let Some(label_id) = state.selected_label.as_ref() {
            return Some(Self::Label(label_id.clone()));
        }

        let selected_folder = state.selected_folder.as_ref()?;
        if let Some(kind) = selected_folder.strip_prefix("smart:") {
            Some(Self::SmartFolder(kind.to_string()))
        } else {
            Some(Self::Folder(selected_folder.clone()))
        }
    }

    pub(crate) fn apply_selection(&self, state: &mut NativeAppState) {
        state.selected_view = SelectedView::Mail;
        match self {
            Self::Folder(folder_id) => {
                state.selected_folder = Some(folder_id.clone());
                state.selected_label = None;
            }
            Self::Label(label_id) => {
                state.selected_folder = None;
                state.selected_label = Some(label_id.clone());
            }
            Self::SmartFolder(kind) => {
                state.selected_folder = Some(format!("smart:{kind}"));
                state.selected_label = None;
            }
            Self::Search(query) => {
                if state.search.submitted_query.is_none() {
                    state.search.previous_source = Self::route_from_state(state);
                }
                state.search.query = query.clone();
                state.search.submitted_query = Some(query.clone());
                state.search.condition_group = None;
                state.selected_folder = None;
                state.selected_label = None;
            }
            Self::AdvancedSearch(group) => {
                if state.search.condition_group.is_none() {
                    state.search.previous_source = Self::route_from_state(state);
                }
                state.search.query.clear();
                state.search.submitted_query = None;
                state.search.condition_group = Some(group.clone());
                state.selected_folder = None;
                state.selected_label = None;
            }
        }

        if !matches!(self, Self::Search(_) | Self::AdvancedSearch(_)) {
            state.search = SearchState::default();
        }
    }

    pub(crate) fn load_context(&self) -> String {
        match self {
            Self::Folder(folder_id) => format!("list messages for {folder_id}"),
            Self::Label(label_id) => format!("list messages for label {label_id}"),
            Self::SmartFolder(kind) => format!("list smart folder {kind}"),
            Self::Search(query) => format!("search messages for {query}"),
            Self::AdvancedSearch(_) => "advanced search messages".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedMessageCollection {
    pub(crate) messages: Vec<Message>,
    pub(crate) calendar_events: Vec<CalendarSearchResult>,
    pub(crate) selected_message: Option<String>,
    pub(crate) selected_full_message: Option<FullMessage>,
    pub(crate) selected_message_labels: Vec<Label>,
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedFullMessage {
    pub(crate) message: FullMessage,
    pub(crate) labels: Vec<Label>,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeSyncRefreshRequest {
    pub(crate) accounts_request: ResourceRequest,
    pub(crate) messages_request: Option<ResourceRequest>,
    pub(crate) source: Option<NativeMessageSource>,
    pub(crate) selected_message: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedSyncRefresh {
    pub(crate) accounts: Vec<Account>,
    pub(crate) folders_by_account: BTreeMap<String, Vec<Folder>>,
    pub(crate) labels: Vec<Label>,
    pub(crate) messages: Option<LoadedMessageCollection>,
    /// What a folder move did, message by message; `None` for other actions.
    pub(crate) move_summary: Option<MoveSummary>,
}

/// A message that was moved, as the server now knows it: its id in the
/// destination folder and the folder the move left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MovedRef {
    pub(crate) message_id: String,
    pub(crate) original_folder_id: String,
}

/// The per-message outcome of a folder move. A failure does not stop the
/// messages after it: each is tried, and the ones that moved can be moved back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MoveSummary {
    /// Moved, and the server said where, so each can be moved back.
    pub(crate) moved: Vec<MovedRef>,
    /// Moved, but the server did not say where, so these cannot be moved back.
    pub(crate) moved_untracked: usize,
    /// (message id, error) for each message that did not move.
    pub(crate) failed: Vec<(String, String)>,
    /// This summary reports an undo, which is not itself undoable.
    pub(crate) is_undo: bool,
}

impl MoveSummary {
    pub(crate) fn moved_count(&self) -> usize {
        self.moved.len() + self.moved_untracked
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.is_undo && !self.moved.is_empty()
    }

    pub(crate) fn text(&self) -> String {
        let moved = self.moved_count();
        let failed = self.failed.len();
        let total = moved + failed;
        let noun = |n: usize| if n == 1 { "message" } else { "messages" };
        let verb = if self.is_undo { "Moved back" } else { "Moved" };
        let mut text = if failed == 0 {
            format!("{verb} {moved} {}.", noun(moved))
        } else if moved == 0 {
            format!("Could not move {failed} {}", noun(failed))
        } else {
            format!("{verb} {moved} of {total} messages; {failed} failed")
        };
        if let Some((_, error)) = self.failed.first() {
            text.push_str(&format!(": {error}"));
            if failed > 1 {
                text.push_str(&format!(" (and {} more)", failed - 1));
            }
        }
        if self.moved_untracked > 0 && !self.is_undo {
            text.push_str(&format!(
                " Undo is unavailable for {} that the server did not locate.",
                self.moved_untracked
            ));
        }
        text
    }
}

/// The last move's outcome, shown until dismissed or expired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveNotice {
    pub(crate) summary: MoveSummary,
    pub(crate) expires_at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeMessageMutation {
    Archive {
        message_id: String,
    },
    Delete {
        message_id: String,
    },
    SetRead {
        message_id: String,
        is_read: bool,
    },
    SetFlagged {
        message_id: String,
        is_flagged: bool,
    },
    Snooze {
        message_id: String,
        until: String,
    },
    /// Apply one label to every listed message (drag-drop and "Move to...").
    AddLabels {
        message_ids: Vec<String>,
        label_id: String,
    },
    /// Move every listed message to one folder (drag-drop and "Move to...").
    MoveToFolder {
        message_ids: Vec<String>,
        folder_id: String,
        /// Each message's folder before the move: id -> folder id.
        source_folders: BTreeMap<String, String>,
    },
    /// Move messages back after a completed move (the undo of `MoveToFolder`).
    UndoMove {
        moves: Vec<MovedRef>,
    },
}

impl NativeMessageMutation {
    pub(crate) fn message_id(&self) -> &str {
        match self {
            Self::Archive { message_id }
            | Self::Delete { message_id }
            | Self::SetRead { message_id, .. }
            | Self::SetFlagged { message_id, .. }
            | Self::Snooze { message_id, .. } => message_id,
            Self::AddLabels { message_ids, .. } | Self::MoveToFolder { message_ids, .. } => {
                message_ids.first().map(String::as_str).unwrap_or_default()
            }
            Self::UndoMove { moves } => moves
                .first()
                .map(|moved| moved.message_id.as_str())
                .unwrap_or_default(),
        }
    }

    pub(crate) fn action_label(&self) -> &'static str {
        match self {
            Self::Archive { .. } => "archive message",
            Self::Delete { .. } => "delete message",
            Self::SetRead { is_read: true, .. } => "mark message read",
            Self::SetRead { is_read: false, .. } => "mark message unread",
            Self::SetFlagged {
                is_flagged: true, ..
            } => "flag message",
            Self::SetFlagged {
                is_flagged: false, ..
            } => "unflag message",
            Self::Snooze { .. } => "snooze message",
            Self::AddLabels { .. } => "apply label",
            Self::MoveToFolder { .. } => "move message to folder",
            Self::UndoMove { .. } => "undo move",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeDraftSeed {
    pub(crate) recipients: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposeRecipientField {
    To,
    Cc,
    Bcc,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeLabelMutation {
    Add {
        message_id: String,
        label_id: String,
    },
    Remove {
        message_id: String,
        label_id: String,
    },
}

impl NativeLabelMutation {
    pub(crate) fn message_id(&self) -> &str {
        match self {
            Self::Add { message_id, .. } | Self::Remove { message_id, .. } => message_id,
        }
    }
}

pub(crate) fn start_message_collection_load(
    state_entity: &Entity<NativeAppState>,
    source: NativeMessageSource,
    app_cx: &mut App,
) {
    start_message_collection_load_with_preferred(state_entity, source, None, app_cx);
}

pub(crate) fn start_message_collection_load_with_preferred(
    state_entity: &Entity<NativeAppState>,
    source: NativeMessageSource,
    preferred_message_id: Option<String>,
    app_cx: &mut App,
) {
    let load_source = source.clone();
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        let request = begin_message_collection_load(state, &source);
        sync_window_title(state, cx);
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { load_message_collection(base_url, load_source, preferred_message_id) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_message_collection(state, request, loaded);
                        }
                        Err(err) => {
                            reject_message_collection_load(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn begin_message_collection_load(
    state: &mut NativeAppState,
    source: &NativeMessageSource,
) -> ResourceRequest {
    let previous_source = NativeMessageSource::from_state(state);
    source.apply_selection(state);
    if previous_source.as_ref() != Some(source) {
        state.expanded_thread_keys.clear();
    }
    state.messages.clear();
    state.selected_message = None;
    state.selected_message_ids.clear();
    state.selected_full_message = None;
    state.selected_message_resource.reset();
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
    state.attachment_busy = false;
    state.attachment_notice = None;
    state.attachment_error = None;
    state.search_event_results.clear();
    state.last_error = None;
    state.messages_resource.begin_load()
}

pub(crate) fn load_message_collection(
    base_url: String,
    source: NativeMessageSource,
    preferred_message_id: Option<String>,
) -> Result<LoadedMessageCollection> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        load_message_collection_with_client(&client, source, preferred_message_id).await
    })
}

pub(crate) async fn load_message_collection_with_client(
    client: &EmailClient,
    source: NativeMessageSource,
    preferred_message_id: Option<String>,
) -> Result<LoadedMessageCollection> {
    let loaded = match &source {
        NativeMessageSource::Folder(folder_id) => client
            .list_folder_messages(folder_id)
            .await
            .map(|messages| (messages, Vec::<CalendarSearchResult>::new())),
        NativeMessageSource::Label(label_id) => client
            .list_label_messages(label_id)
            .await
            .map(|messages| (messages, Vec::<CalendarSearchResult>::new())),
        NativeMessageSource::SmartFolder(kind) => client
            .list_smart_messages(kind)
            .await
            .map(|messages| (messages, Vec::<CalendarSearchResult>::new())),
        NativeMessageSource::Search(query) => client
            .search(query, None)
            .await
            .map(|results| (results.messages, results.events)),
        NativeMessageSource::AdvancedSearch(group) => client
            .advanced_search(group)
            .await
            .map(|results| (results.messages, Vec::<CalendarSearchResult>::new())),
    };
    let (mut messages, calendar_events) = loaded.with_context(|| source.load_context())?;

    let selected_message = preferred_message_id
        .filter(|message_id| messages.iter().any(|message| message.id == *message_id))
        .or_else(|| messages.first().map(|message| message.id.clone()));
    let (selected_full_message, selected_message_labels) =
        if let Some(message_id) = selected_message.as_deref() {
            match client.get_message(message_id).await {
                Ok(full_message) => {
                    update_summary_from_full_message(&mut messages, &full_message);
                    let labels = client
                        .get_message_labels(message_id)
                        .await
                        .with_context(|| format!("get labels for selected message {message_id}"))?;
                    (Some(full_message), labels)
                }
                Err(err) => {
                    tracing::warn!("failed to load selected message {message_id}: {err}");
                    (None, Vec::new())
                }
            }
        } else {
            (None, Vec::new())
        };

    Ok(LoadedMessageCollection {
        messages,
        calendar_events,
        selected_message,
        selected_full_message,
        selected_message_labels,
    })
}

pub(crate) fn apply_loaded_message_collection(
    state: &mut NativeAppState,
    request: ResourceRequest,
    loaded: LoadedMessageCollection,
) -> bool {
    if !state
        .messages_resource
        .resolve(request, loaded.messages.clone())
    {
        return false;
    }

    state.messages = loaded.messages;
    state.search_event_results = loaded.calendar_events;
    retain_existing_message_selection(state);
    retain_existing_thread_expansions(state);
    retain_existing_stack_expansions(state);
    state.selected_message = loaded
        .selected_message
        .or_else(|| state.messages.first().map(|message| message.id.clone()));
    state.selected_full_message = None;
    state.selected_message_resource.reset();
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
    state.attachment_busy = false;
    state.attachment_notice = None;
    state.attachment_error = None;

    if let Some(full_message) = loaded.selected_full_message {
        state.selected_message = Some(full_message.id.clone());
        state.selected_message_resource = AsyncResource::ready(full_message.clone());
        apply_full_message(state, full_message);
        state.selected_message_labels = loaded.selected_message_labels.clone();
        state.selected_message_labels_resource =
            AsyncResource::ready(loaded.selected_message_labels);
    }

    state.last_error = None;
    true
}

pub(crate) fn reject_message_collection_load(
    state: &mut NativeAppState,
    request: ResourceRequest,
    err: anyhow::Error,
) -> bool {
    let message = err.to_string();
    if !state.messages_resource.reject(request, message.clone()) {
        return false;
    }

    state.messages.clear();
    state.selected_message = None;
    state.selected_message_ids.clear();
    state.expanded_thread_keys.clear();
    state.selected_full_message = None;
    state.selected_message_resource.reset();
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
    state.attachment_busy = false;
    state.attachment_notice = None;
    state.attachment_error = None;
    state.search_event_results.clear();
    state.last_error = Some(message);
    true
}

pub(crate) fn start_sync_complete_refresh(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        let request = begin_sync_complete_refresh(state);
        sync_window_title(state, cx);
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let load_request = request.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_sync_complete_refresh(base_url, load_request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn start_mark_folder_read(
    state_entity: &Entity<NativeAppState>,
    folder_id: String,
    app_cx: &mut App,
) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        let request = begin_sync_complete_refresh(state);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let load_request = request.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { mark_folder_read_and_refresh(base_url, folder_id, load_request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn start_message_mutation_and_refresh(
    state_entity: &Entity<NativeAppState>,
    mutation: NativeMessageMutation,
    app_cx: &mut App,
) {
    if matches!(
        mutation,
        NativeMessageMutation::Archive { .. }
            | NativeMessageMutation::Delete { .. }
            | NativeMessageMutation::AddLabels { .. }
            | NativeMessageMutation::MoveToFolder { .. }
    ) {
        let previous = state_entity.update(app_cx, |state, cx| {
            let previous = replace_pending_undo(
                &mut state.pending_undo,
                PendingUndo {
                    mutation,
                    expires_at: Instant::now()
                        + Duration::from_secs(clamp_undo_window(state.undo_window_secs).into()),
                },
            );
            cx.notify();
            previous
        });
        // There is only one visible undo slot.  Do not silently lose the
        // mutation that occupied it when a second action arrives.
        if let Some(previous) = previous {
            commit_message_mutation_and_refresh(state_entity, previous, app_cx);
        }
        return;
    }
    commit_message_mutation_and_refresh(state_entity, mutation, app_cx);
}

pub(crate) fn replace_pending_undo(
    pending: &mut Option<PendingUndo>,
    replacement: PendingUndo,
) -> Option<NativeMessageMutation> {
    pending.replace(replacement).map(|pending| pending.mutation)
}

pub(crate) fn take_expired_pending_undo(
    pending: &mut Option<PendingUndo>,
    now: Instant,
) -> Option<NativeMessageMutation> {
    if pending
        .as_ref()
        .is_some_and(|pending| now >= pending.expires_at)
    {
        return pending.take().map(|pending| pending.mutation);
    }
    None
}

pub(crate) fn commit_message_mutation_and_refresh(
    state_entity: &Entity<NativeAppState>,
    mutation: NativeMessageMutation,
    app_cx: &mut App,
) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        let request = begin_sync_complete_refresh(state);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let load_request = request.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { mutate_message_and_refresh(base_url, mutation, load_request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                });
            }
        })
        .detach();
}

/// How long the outcome of a move, and its undo, stay on screen.
pub(crate) const MOVE_NOTICE_LIFETIME: Duration = Duration::from_secs(30);

/// Move the messages of the last completed move back where they came from.
pub(crate) fn start_undo_completed_move(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let moves = state_entity.update(app_cx, |state, cx| {
        let notice = state.move_notice.take();
        cx.notify();
        notice
            .filter(|notice| notice.summary.can_undo())
            .map(|notice| notice.summary.moved)
    });
    if let Some(moves) = moves {
        commit_message_mutation_and_refresh(
            state_entity,
            NativeMessageMutation::UndoMove { moves },
            app_cx,
        );
    }
}

pub(crate) fn dismiss_move_notice(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.move_notice = None;
        cx.notify();
    });
}

pub(crate) fn expire_move_notice(notice: &mut Option<MoveNotice>, now: Instant) -> bool {
    if notice
        .as_ref()
        .is_some_and(|notice| now >= notice.expires_at)
    {
        *notice = None;
        return true;
    }
    false
}

pub(crate) fn cancel_pending_undo(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.pending_undo = None;
        cx.notify();
    });
}

/// Esc while a delayed mutation is pending cancels it instead of its usual
/// meaning (clearing the selection). Returns whether anything was cancelled.
pub(crate) fn cancel_pending_undo_on_escape(
    state_entity: &Entity<NativeAppState>,
    app_cx: &mut App,
) -> bool {
    if state_entity.read(app_cx).pending_undo.is_none() {
        return false;
    }
    cancel_pending_undo(state_entity, app_cx);
    true
}

/// Keyboard undo (`z`): cancels the pending delayed mutation, else reverses
/// the last completed move, else cancels a just-sent message. Returns whether
/// anything was undone.
pub(crate) fn undo_latest_action(state_entity: &Entity<NativeAppState>, app_cx: &mut App) -> bool {
    if cancel_pending_undo_on_escape(state_entity, app_cx) {
        return true;
    }
    let (can_undo_move, can_cancel_send) = {
        let state = state_entity.read(app_cx);
        (
            state
                .move_notice
                .as_ref()
                .is_some_and(|notice| notice.summary.can_undo()),
            state
                .pending_send
                .as_ref()
                .is_some_and(|send| !send.cancelling),
        )
    };
    if can_undo_move {
        start_undo_completed_move(state_entity, app_cx);
    } else if can_cancel_send {
        start_cancel_send(state_entity, app_cx);
    } else {
        return false;
    }
    true
}

pub(crate) fn take_expired_undo(
    state_entity: &Entity<NativeAppState>,
    app_cx: &mut App,
) -> Option<NativeMessageMutation> {
    state_entity.update(app_cx, |state, cx| {
        let expired = take_expired_pending_undo(&mut state.pending_undo, Instant::now());
        if expired.is_some() {
            cx.notify();
        }
        expired
    })
}

pub(crate) fn start_message_label_mutation(
    state_entity: &Entity<NativeAppState>,
    mutation: NativeLabelMutation,
    app_cx: &mut App,
) {
    let message_id = mutation.message_id().to_string();
    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        if state.selected_message.as_deref() != Some(message_id.as_str()) {
            return None;
        }
        state.last_error = None;
        let request = state.selected_message_labels_resource.begin_load();
        cx.notify();
        Some((state.api_base_url.clone(), request))
    }) else {
        return;
    };

    let task = app_cx
        .background_executor()
        .spawn(async move { mutate_message_label_and_reload(base_url, mutation) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    if state.selected_message.as_deref() != Some(message_id.as_str()) {
                        return;
                    }
                    match result {
                        Ok(labels) => {
                            if state
                                .selected_message_labels_resource
                                .resolve(request, labels.clone())
                            {
                                state.selected_message_labels = labels;
                                state.last_error = None;
                            }
                        }
                        Err(err) => {
                            let message = err.to_string();
                            if state
                                .selected_message_labels_resource
                                .reject(request, message.clone())
                            {
                                state.last_error = Some(message);
                            }
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn start_bulk_message_action(
    state_entity: &Entity<NativeAppState>,
    action: BulkAction,
    app_cx: &mut App,
) {
    let Some((base_url, request, ids)) = state_entity.update(app_cx, |state, cx| {
        let ids = selected_action_message_ids(state, None);
        if ids.is_empty() {
            return None;
        }
        let request = begin_sync_complete_refresh(state);
        state.selected_message_ids.clear();
        state.last_error = None;
        cx.notify();
        Some((state.api_base_url.clone(), request, ids))
    }) else {
        return;
    };

    let load_request = request.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { bulk_message_action_and_refresh(base_url, ids, action, load_request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(outcome) => {
                            apply_loaded_sync_refresh(state, request, outcome.loaded);
                            state.last_error = outcome.warning;
                        }
                        Err(err) => {
                            reject_sync_refresh(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn begin_sync_complete_refresh(state: &mut NativeAppState) -> NativeSyncRefreshRequest {
    let source = NativeMessageSource::from_state(state);
    let selected_message = state.selected_message.clone();
    let accounts_request = state.accounts_resource.begin_load();
    let messages_request = source
        .as_ref()
        .map(|_| state.messages_resource.begin_load());
    state.last_error = None;

    NativeSyncRefreshRequest {
        accounts_request,
        messages_request,
        source,
        selected_message,
    }
}

pub(crate) fn load_sync_complete_refresh(
    base_url: String,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let tree = load_account_tree(&client).await?;
        let messages = if let Some(source) = request.source {
            Some(
                load_message_collection_with_client(&client, source, request.selected_message)
                    .await?,
            )
        } else {
            None
        };

        Ok(LoadedSyncRefresh {
            accounts: tree.accounts,
            folders_by_account: tree.folders_by_account,
            labels: tree.labels,
            messages,
            move_summary: None,
        })
    })
}

pub(crate) fn mark_folder_read_and_refresh(
    base_url: String,
    folder_id: String,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .mark_folder_read(&folder_id)
            .await
            .with_context(|| format!("mark folder read {folder_id}"))?;
        let tree = load_account_tree(&client).await?;
        let messages = if let Some(source) = request.source {
            Some(
                load_message_collection_with_client(&client, source, request.selected_message)
                    .await?,
            )
        } else {
            None
        };

        Ok(LoadedSyncRefresh {
            accounts: tree.accounts,
            folders_by_account: tree.folders_by_account,
            labels: tree.labels,
            messages,
            move_summary: None,
        })
    })
}

pub(crate) fn mutate_message_and_refresh(
    base_url: String,
    mutation: NativeMessageMutation,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let action_label = mutation.action_label();
        let mut move_summary = None;
        match &mutation {
            NativeMessageMutation::Archive { message_id } => {
                client.archive_message(message_id).await?;
            }
            NativeMessageMutation::Delete { message_id } => {
                client.delete_message(message_id).await?;
            }
            NativeMessageMutation::SetRead {
                message_id,
                is_read,
            } => {
                client
                    .patch_message(
                        message_id,
                        &PatchMessageRequest {
                            is_read: Some(*is_read),
                            is_flagged: None,
                        },
                    )
                    .await?;
            }
            NativeMessageMutation::SetFlagged {
                message_id,
                is_flagged,
            } => {
                client
                    .patch_message(
                        message_id,
                        &PatchMessageRequest {
                            is_read: None,
                            is_flagged: Some(*is_flagged),
                        },
                    )
                    .await?;
            }
            NativeMessageMutation::Snooze { message_id, until } => {
                client
                    .snooze_message(
                        message_id,
                        &SnoozeRequest {
                            until: until.clone(),
                        },
                    )
                    .await?;
            }
            NativeMessageMutation::AddLabels {
                message_ids,
                label_id,
            } => {
                for message_id in message_ids {
                    client
                        .add_message_label(message_id, label_id)
                        .await
                        .with_context(|| format!("add label {label_id} to message {message_id}"))?;
                }
            }
            NativeMessageMutation::MoveToFolder {
                message_ids,
                folder_id,
                source_folders,
            } => {
                let items = message_ids
                    .iter()
                    .map(|message_id| {
                        (
                            message_id.clone(),
                            folder_id.clone(),
                            source_folders.get(message_id).cloned().unwrap_or_default(),
                        )
                    })
                    .collect();
                move_summary = Some(move_each(&client, items, false).await);
            }
            NativeMessageMutation::UndoMove { moves } => {
                let items = moves
                    .iter()
                    .map(|moved| {
                        (
                            moved.message_id.clone(),
                            moved.original_folder_id.clone(),
                            String::new(),
                        )
                    })
                    .collect();
                move_summary = Some(move_each(&client, items, true).await);
            }
        }

        let tree = load_account_tree(&client)
            .await
            .with_context(|| format!("refresh after {action_label}"))?;
        let messages = if let Some(source) = request.source {
            Some(
                load_message_collection_with_client(&client, source, request.selected_message)
                    .await
                    .with_context(|| format!("reload active mailbox after {action_label}"))?,
            )
        } else {
            None
        };

        tracing::info!(
            "completed native {action_label} for {}",
            mutation.message_id()
        );
        Ok(LoadedSyncRefresh {
            accounts: tree.accounts,
            folders_by_account: tree.folders_by_account,
            labels: tree.labels,
            messages,
            move_summary,
        })
    })
}

/// Move each `(message id, target folder, original folder)`, recording every
/// outcome rather than stopping at the first failure.
pub(crate) async fn move_each(
    client: &EmailClient,
    items: Vec<(String, String, String)>,
    is_undo: bool,
) -> MoveSummary {
    let mut summary = MoveSummary {
        is_undo,
        ..MoveSummary::default()
    };
    for (message_id, target_folder, original_folder) in items {
        match client.move_message(&message_id, &target_folder).await {
            Ok(response) => match response.message_id {
                Some(new_id) => summary.moved.push(MovedRef {
                    message_id: new_id,
                    original_folder_id: original_folder,
                }),
                None => summary.moved_untracked += 1,
            },
            Err(err) => summary.failed.push((message_id, format!("{err:#}"))),
        }
    }
    summary
}

pub(crate) struct BulkRefreshOutcome {
    pub(crate) loaded: LoadedSyncRefresh,
    pub(crate) warning: Option<String>,
}

pub(crate) fn bulk_message_action_and_refresh(
    base_url: String,
    ids: Vec<String>,
    action: BulkAction,
    request: NativeSyncRefreshRequest,
) -> Result<BulkRefreshOutcome> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let action_label = bulk_action_label(&action);
        let response = client
            .bulk_messages(&BulkRequest { ids, action })
            .await
            .with_context(|| format!("bulk {action_label}"))?;
        let warning = (response.failed > 0).then(|| {
            let details = response
                .results
                .iter()
                .filter_map(|result| result.error.as_deref())
                .collect::<Vec<_>>()
                .join("; ");
            format!(
                "Bulk {action_label}: {} succeeded, {} failed: {details}",
                response.processed, response.failed
            )
        });

        let tree = load_account_tree(&client)
            .await
            .with_context(|| format!("refresh after bulk {action_label}"))?;
        let messages = if let Some(source) = request.source {
            Some(
                load_message_collection_with_client(&client, source, request.selected_message)
                    .await
                    .with_context(|| format!("reload active mailbox after bulk {action_label}"))?,
            )
        } else {
            None
        };

        Ok(BulkRefreshOutcome {
            loaded: LoadedSyncRefresh {
                accounts: tree.accounts,
                folders_by_account: tree.folders_by_account,
                labels: tree.labels,
                messages,
                move_summary: None,
            },
            warning,
        })
    })
}

pub(crate) fn bulk_action_label(action: &BulkAction) -> &'static str {
    match action {
        BulkAction::Archive => "archive",
        BulkAction::Delete => "delete",
        BulkAction::MarkRead => "mark read",
        BulkAction::MarkUnread => "mark unread",
    }
}

pub(crate) fn mutate_message_label_and_reload(
    base_url: String,
    mutation: NativeLabelMutation,
) -> Result<Vec<Label>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        match &mutation {
            NativeLabelMutation::Add {
                message_id,
                label_id,
            } => {
                client
                    .add_message_label(message_id, label_id)
                    .await
                    .with_context(|| format!("add label {label_id} to message {message_id}"))?;
            }
            NativeLabelMutation::Remove {
                message_id,
                label_id,
            } => {
                client
                    .remove_message_label(message_id, label_id)
                    .await
                    .with_context(|| {
                        format!("remove label {label_id} from message {message_id}")
                    })?;
            }
        }

        client
            .get_message_labels(mutation.message_id())
            .await
            .with_context(|| format!("reload labels for message {}", mutation.message_id()))
    })
}

pub(crate) fn apply_loaded_sync_refresh(
    state: &mut NativeAppState,
    request: NativeSyncRefreshRequest,
    mut loaded: LoadedSyncRefresh,
) -> bool {
    // The move already happened on the server, so its outcome (and the ids
    // needed to undo it) is kept even when the refresh itself is stale.
    if let Some(summary) = loaded.move_summary.take() {
        state.move_notice = Some(MoveNotice {
            summary,
            expires_at: Instant::now() + MOVE_NOTICE_LIFETIME,
        });
    }
    if !state
        .accounts_resource
        .resolve(request.accounts_request, loaded.accounts.clone())
    {
        return false;
    }

    state.accounts = loaded.accounts;
    state.folders_by_account = loaded.folders_by_account;
    state.labels = loaded.labels;

    if let (Some(messages_request), Some(messages)) = (request.messages_request, loaded.messages) {
        apply_loaded_message_collection(state, messages_request, messages);
    }

    state.last_error = None;
    true
}

pub(crate) fn reject_sync_refresh(
    state: &mut NativeAppState,
    request: NativeSyncRefreshRequest,
    err: anyhow::Error,
) -> bool {
    let message = err.to_string();
    let accounts_rejected = state
        .accounts_resource
        .reject(request.accounts_request, message.clone());
    let messages_rejected = request.messages_request.is_some_and(|messages_request| {
        state
            .messages_resource
            .reject(messages_request, message.clone())
    });

    if !accounts_rejected && !messages_rejected {
        return false;
    }

    state.last_error = Some(message);
    true
}

pub(crate) fn start_full_message_load(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    app_cx: &mut App,
) {
    let message_id = message_id.to_string();
    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        let request = begin_full_message_load(state, &message_id)?;
        sync_window_title(state, cx);
        cx.notify();
        Some((state.api_base_url.clone(), request))
    }) else {
        return;
    };

    let load_message_id = message_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_full_message(base_url, load_message_id) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_full_message(state, request, &message_id, loaded);
                        }
                        Err(err) => {
                            reject_full_message_load(state, request, &message_id, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn begin_full_message_load(
    state: &mut NativeAppState,
    message_id: &str,
) -> Option<ResourceRequest> {
    if state.selected_full_message.as_ref().is_some_and(|message| {
        message.id == message_id && state.selected_message_resource.is_ready()
    }) {
        return None;
    }

    if state.selected_message.as_deref() != Some(message_id) {
        reset_compose_state(state, None);
    }
    state.selected_message = Some(message_id.to_string());
    state.selected_full_message = None;
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
    state.attachment_busy = false;
    state.attachment_notice = None;
    state.attachment_error = None;
    state.pending_external_link = None;
    // Authentication results are available only on the full message. The
    // loaded-message path applies the allowlist decision authoritatively.
    state.show_external_images = false;
    state.last_error = None;
    Some(state.selected_message_resource.begin_load())
}

pub(crate) fn load_full_message(base_url: String, message_id: String) -> Result<LoadedFullMessage> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let message = client
            .get_message(&message_id)
            .await
            .with_context(|| format!("get message {message_id}"))?;
        let labels = client
            .get_message_labels(&message_id)
            .await
            .with_context(|| format!("get labels for message {message_id}"))?;
        Ok(LoadedFullMessage { message, labels })
    })
}

pub(crate) fn apply_loaded_full_message(
    state: &mut NativeAppState,
    request: ResourceRequest,
    message_id: &str,
    loaded: LoadedFullMessage,
) -> bool {
    if state.selected_message.as_deref() != Some(message_id) {
        return false;
    }
    if !state
        .selected_message_resource
        .resolve(request, loaded.message.clone())
    {
        return false;
    }

    apply_full_message(state, loaded.message);
    state.selected_message_labels = loaded.labels.clone();
    state.selected_message_labels_resource = AsyncResource::ready(loaded.labels);
    state.last_error = None;
    true
}

pub(crate) fn reject_full_message_load(
    state: &mut NativeAppState,
    request: ResourceRequest,
    message_id: &str,
    err: anyhow::Error,
) -> bool {
    if state.selected_message.as_deref() != Some(message_id) {
        return false;
    }

    let message = err.to_string();
    if !state
        .selected_message_resource
        .reject(request, message.clone())
    {
        return false;
    }

    state.selected_full_message = None;
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
    state.attachment_busy = false;
    state.attachment_notice = None;
    state.attachment_error = None;
    state.last_error = Some(message);
    true
}

pub(crate) fn update_summary_from_full_message(
    messages: &mut [Message],
    full_message: &FullMessage,
) {
    if let Some(summary) = messages
        .iter_mut()
        .find(|message| message.id == full_message.id)
    {
        summary.is_read = full_message.is_read;
        summary.is_flagged = full_message.is_flagged;
    }
}

pub(crate) fn apply_full_message(state: &mut NativeAppState, full_message: FullMessage) {
    let was_unread = state
        .messages
        .iter()
        .find(|message| message.id == full_message.id)
        .is_some_and(|message| !message.is_read);
    update_summary_from_full_message(&mut state.messages, &full_message);
    if was_unread && full_message.is_read {
        decrement_folder_unread(&mut state.folders_by_account, &full_message.folder_id);
    }
    state.selected_message = Some(full_message.id.clone());
    state.show_external_images = full_message
        .from_email
        .as_deref()
        .is_some_and(|email| sender_images_allowed_for_message(state, &full_message, email));
    state.selected_full_message = Some(full_message);
    state.attachment_notice = None;
    state.attachment_error = None;
}

pub(crate) fn decrement_folder_unread(
    folders_by_account: &mut BTreeMap<String, Vec<Folder>>,
    folder_id: &str,
) {
    for folder in folders_by_account
        .values_mut()
        .flat_map(|folders| folders.iter_mut())
    {
        if folder.id == folder_id {
            folder.unread_count = folder.unread_count.saturating_sub(1);
            break;
        }
    }
}

pub(crate) fn selected_action_message_ids(
    state: &NativeAppState,
    fallback_message_id: Option<&str>,
) -> Vec<String> {
    let visible = state
        .messages
        .iter()
        .map(|message| message.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();

    for id in &state.selected_message_ids {
        if visible.contains(id.as_str()) && seen.insert(id.clone()) {
            ids.push(id.clone());
        }
    }

    if ids.is_empty() {
        if let Some(id) = fallback_message_id {
            if visible.contains(id) {
                ids.push(id.to_string());
            }
        }
    }

    ids
}

pub(crate) fn toggle_message_selection(state: &mut NativeAppState, message_id: &str) {
    if let Some(pos) = state
        .selected_message_ids
        .iter()
        .position(|selected_id| selected_id == message_id)
    {
        state.selected_message_ids.remove(pos);
    } else if state
        .messages
        .iter()
        .any(|message| message.id == message_id)
    {
        state.selected_message_ids.push(message_id.to_string());
    }
}

pub(crate) fn set_all_message_selection(state: &mut NativeAppState, selected: bool) {
    if selected {
        state.selected_message_ids = state
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect();
    } else {
        state.selected_message_ids.clear();
    }
}

pub(crate) fn retain_existing_message_selection(state: &mut NativeAppState) {
    let visible = state
        .messages
        .iter()
        .map(|message| message.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    state
        .selected_message_ids
        .retain(|id| visible.contains(id.as_str()) && seen.insert(id.clone()));
}

pub(crate) fn message_select_all_state(state: &NativeAppState) -> CheckboxState {
    if state.messages.is_empty() || state.selected_message_ids.is_empty() {
        return CheckboxState::Unchecked;
    }

    let visible_count = state.messages.len();
    let selected_visible_count = selected_action_message_ids(state, None).len();
    if selected_visible_count == 0 {
        CheckboxState::Unchecked
    } else if selected_visible_count == visible_count {
        CheckboxState::Checked
    } else {
        CheckboxState::Indeterminate
    }
}

#[cfg(test)]
mod undo_key_tests {
    use super::*;

    fn pending() -> PendingUndo {
        PendingUndo {
            mutation: NativeMessageMutation::Archive {
                message_id: "m1".into(),
            },
            expires_at: Instant::now() + Duration::from_secs(5),
        }
    }

    fn state_with_pending() -> NativeAppState {
        NativeAppState {
            pending_undo: Some(pending()),
            ..Default::default()
        }
    }

    #[gpui::test]
    fn escape_cancels_the_pending_mutation(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| state_with_pending());
        assert!(cx.update(|app| cancel_pending_undo_on_escape(&entity, app)));
        entity.update(cx, |state, _| assert!(state.pending_undo.is_none()));
    }

    #[gpui::test]
    fn escape_without_a_pending_mutation_is_not_consumed(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| NativeAppState::default());
        assert!(!cx.update(|app| cancel_pending_undo_on_escape(&entity, app)));
    }

    #[gpui::test]
    fn z_cancels_the_pending_mutation(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| state_with_pending());
        assert!(cx.update(|app| undo_latest_action(&entity, app)));
        entity.update(cx, |state, _| assert!(state.pending_undo.is_none()));
    }

    #[gpui::test]
    fn z_with_nothing_to_undo_is_not_consumed(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| NativeAppState::default());
        assert!(!cx.update(|app| undo_latest_action(&entity, app)));
    }

    #[gpui::test]
    fn message_list_keys_undo_even_with_no_rows(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| state_with_pending());
        assert!(cx.update(|app| handle_message_list_key(&entity, &[], None, "z", false, app)));
        entity.update(cx, |state, _| assert!(state.pending_undo.is_none()));

        entity.update(cx, |state, _| state.pending_undo = Some(pending()));
        entity.update(cx, |state, _| state.selected_message_ids.push("m1".into()));
        assert!(cx.update(|app| handle_message_list_key(
            &entity,
            &["m1".to_string()],
            Some("m1"),
            "escape",
            false,
            app
        )));
        entity.update(cx, |state, _| {
            assert!(state.pending_undo.is_none());
            assert!(
                !state.selected_message_ids.is_empty(),
                "first Esc cancels the action, not the selection"
            );
        });
    }

    #[gpui::test]
    fn reading_pane_escape_and_z_cancel_before_clearing(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| state_with_pending());
        assert!(cx.update(|app| handle_reading_pane_key(&entity, "escape", false, false, app)));
        entity.update(cx, |state, _| assert!(state.pending_undo.is_none()));
        entity.update(cx, |state, _| state.pending_undo = Some(pending()));
        assert!(cx.update(|app| handle_reading_pane_key(&entity, "z", false, false, app)));
        entity.update(cx, |state, _| assert!(state.pending_undo.is_none()));
    }

    #[test]
    fn z_maps_to_undo_in_the_reading_pane() {
        assert_eq!(
            reading_pane_key_action("z", false, false),
            Some(ReadingPaneKeyAction::Undo)
        );
        assert_eq!(reading_pane_key_action("z", false, true), None);
    }
}
