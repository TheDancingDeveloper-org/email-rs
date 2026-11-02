use crate::prelude::*;
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SelectedView {
    Mail,
    Calendar,
}

#[derive(Clone, Debug)]
pub(crate) struct DensityTokens {
    pub(crate) level: u8,
    pub(crate) sidebar_row_height: f32,
    pub(crate) message_row_height: f32,
}

impl DensityTokens {
    pub(crate) fn new(level: u8) -> Self {
        let level = level.min(8);
        let relaxed = level as f32;
        Self {
            level,
            sidebar_row_height: 28.0 + relaxed * 2.0,
            message_row_height: 52.0 + relaxed * 4.0,
        }
    }
}

impl Default for DensityTokens {
    fn default() -> Self {
        Self::new(4)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeLayoutPreferences {
    pub(crate) sidebar_width: u16,
    pub(crate) message_list_width: u16,
}

impl Default for NativeLayoutPreferences {
    fn default() -> Self {
        Self {
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            message_list_width: DEFAULT_MESSAGE_LIST_WIDTH,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativePreferences {
    pub(crate) theme: ThemeMode,
    pub(crate) density_level: u8,
    pub(crate) layout: NativeLayoutPreferences,
}

impl NativePreferences {
    pub(crate) fn from_state(state: &NativeAppState) -> Self {
        Self {
            theme: state.theme.clone(),
            density_level: state.density.level,
            layout: state.layout,
        }
    }

    pub(crate) fn apply_to_state(&self, state: &mut NativeAppState) {
        state.theme = self.theme.clone();
        state.density = DensityTokens::new(self.density_level);
        state.layout = self.layout;
    }
}

impl Default for NativePreferences {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            density_level: DensityTokens::default().level,
            layout: NativeLayoutPreferences::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ComposeState {
    pub(crate) open: bool,
    pub(crate) mode: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) to: Vec<String>,
    pub(crate) cc: Vec<String>,
    pub(crate) bcc: Vec<String>,
    pub(crate) subject: Option<String>,
    pub(crate) body: String,
    pub(crate) in_reply_to: Option<String>,
    pub(crate) quoted_text: Option<String>,
    pub(crate) quoted_from: Option<String>,
    pub(crate) signature: Option<String>,
    pub(crate) signature_loading: bool,
    pub(crate) show_cc: bool,
    pub(crate) show_bcc: bool,
    pub(crate) recipient_suggestions_query: Option<String>,
    pub(crate) recipient_suggestions: Vec<Contact>,
    pub(crate) recipient_suggestions_resource: AsyncResource<Vec<Contact>, String>,
    pub(crate) revision: u64,
    pub(crate) sending: bool,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ModalStates {
    pub(crate) account_setup_open: bool,
    pub(crate) settings_open: bool,
    pub(crate) advanced_search_open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeSearchSuggestion {
    Message(MessageSuggestion),
    Event(CalendarSuggestion),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) submitted_query: Option<String>,
    pub(crate) pending_query: Option<DebouncedSearch>,
    pub(crate) condition_group: Option<ConditionGroup>,
    pub(crate) previous_source: Option<NativeMessageSource>,
    pub(crate) suggestions_query: Option<String>,
    pub(crate) suggestions: Vec<NativeSearchSuggestion>,
    pub(crate) suggestions_open: bool,
    pub(crate) suggestions_resource: AsyncResource<Vec<NativeSearchSuggestion>, String>,
    pub(crate) suggestion_list: SuggestionListState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DebouncedSearch {
    pub(crate) query: String,
    pub(crate) due_at: Instant,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct PaletteState {
    pub(crate) open: bool,
    pub(crate) query: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeSyncStatus {
    pub(crate) connected: bool,
    pub(crate) syncing: bool,
    pub(crate) account_id: Option<String>,
    pub(crate) folder: Option<String>,
    pub(crate) done: i64,
    pub(crate) total: i64,
    pub(crate) last_message: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) completed_at: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeSyncUpdate {
    Connecting,
    Connected,
    Event(SyncEvent),
    Error(String),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct NativeAppState {
    pub(crate) api_base_url: String,
    pub(crate) auth_required: bool,
    pub(crate) theme: ThemeMode,
    pub(crate) density: DensityTokens,
    pub(crate) layout: NativeLayoutPreferences,
    pub(crate) selected_view: SelectedView,
    pub(crate) selected_folder: Option<String>,
    pub(crate) selected_label: Option<String>,
    pub(crate) selected_message: Option<String>,
    pub(crate) selected_message_ids: Vec<String>,
    pub(crate) expanded_thread_keys: BTreeSet<String>,
    pub(crate) selected_full_message: Option<FullMessage>,
    pub(crate) selected_message_resource: AsyncResource<FullMessage, String>,
    pub(crate) selected_message_labels: Vec<Label>,
    pub(crate) selected_message_labels_resource: AsyncResource<Vec<Label>, String>,
    pub(crate) attachment_busy: bool,
    pub(crate) attachment_notice: Option<String>,
    pub(crate) attachment_error: Option<String>,
    pub(crate) pending_unsafe_attachment: Option<String>,
    pub(crate) show_external_images: bool,
    pub(crate) selected_event: Option<String>,
    pub(crate) calendar_week_start: NaiveDate,
    pub(crate) calendar_events: Vec<CalendarEvent>,
    pub(crate) calendar_events_resource: AsyncResource<Vec<CalendarEvent>, String>,
    pub(crate) calendar_event_links_event: Option<String>,
    pub(crate) calendar_event_links: Vec<EventLink>,
    pub(crate) calendar_event_links_resource: AsyncResource<Vec<EventLink>, String>,
    pub(crate) search: SearchState,
    pub(crate) search_event_results: Vec<CalendarSearchResult>,
    pub(crate) messages: Vec<Message>,
    pub(crate) messages_resource: AsyncResource<Vec<Message>, String>,
    pub(crate) accounts: Vec<Account>,
    pub(crate) accounts_resource: AsyncResource<Vec<Account>, String>,
    pub(crate) labels: Vec<Label>,
    pub(crate) folders_by_account: BTreeMap<String, Vec<Folder>>,
    pub(crate) compose: ComposeState,
    pub(crate) modals: ModalStates,
    pub(crate) palette: PaletteState,
    pub(crate) sync: NativeSyncStatus,
    pub(crate) last_error: Option<String>,
}

impl Default for NativeAppState {
    fn default() -> Self {
        Self {
            api_base_url: String::new(),
            auth_required: true,
            theme: ThemeMode::Dark,
            density: DensityTokens::default(),
            layout: NativeLayoutPreferences::default(),
            selected_view: SelectedView::Mail,
            selected_folder: None,
            selected_label: None,
            selected_message: None,
            selected_message_ids: Vec::new(),
            expanded_thread_keys: BTreeSet::new(),
            selected_full_message: None,
            selected_message_resource: AsyncResource::new(),
            selected_message_labels: Vec::new(),
            selected_message_labels_resource: AsyncResource::new(),
            attachment_busy: false,
            attachment_notice: None,
            attachment_error: None,
            pending_unsafe_attachment: None,
            show_external_images: false,
            selected_event: None,
            calendar_week_start: current_calendar_week_start(),
            calendar_events: Vec::new(),
            calendar_events_resource: AsyncResource::new(),
            calendar_event_links_event: None,
            calendar_event_links: Vec::new(),
            calendar_event_links_resource: AsyncResource::new(),
            search: SearchState::default(),
            search_event_results: Vec::new(),
            messages: Vec::new(),
            messages_resource: AsyncResource::new(),
            accounts: Vec::new(),
            accounts_resource: AsyncResource::new(),
            labels: Vec::new(),
            folders_by_account: BTreeMap::new(),
            compose: ComposeState::default(),
            modals: ModalStates::default(),
            palette: PaletteState::default(),
            sync: NativeSyncStatus::default(),
            last_error: None,
        }
    }
}
