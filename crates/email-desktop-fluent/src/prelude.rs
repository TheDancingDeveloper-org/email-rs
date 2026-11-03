//! Shared external imports for the desktop modules.

#![allow(unused_imports)]

pub(crate) use std::borrow::Cow;
pub(crate) use std::collections::{BTreeMap, BTreeSet};
pub(crate) use std::net::TcpStream;
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::process::Command;
pub(crate) use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
pub(crate) use std::thread;
pub(crate) use std::time::{Duration, Instant};

pub(crate) use anyhow::{anyhow, Context as _, Result};
pub(crate) use chrono::{
    Datelike as _, Duration as ChronoDuration, Local, NaiveDate, TimeZone as _, Timelike as _, Utc,
};
pub(crate) use email_client::{set_default_bearer_token, EmailClient};
pub(crate) use email_server::{run_server, Config};
pub(crate) use email_types::{
    Account, AccountSettings, ActionInput, AuthenticationResultStatus, BulkAction, BulkRequest,
    CalendarEvent, CalendarSearchResult, CalendarSuggestion, Condition, ConditionField,
    ConditionGroup, ConditionInput, ConditionOperator, Contact, CreateAccount, CreateWebhook,
    EventLink, Folder, FullMessage, Label, MatchMode, Message, MessageAttachment, MessageBody,
    MessageSuggestion, PatchFolder, PatchMessageRequest, RuleDto, SendRequest, SnoozeRequest,
    SyncEvent, UpdateAccount, UpdateRuleBody, UpdateWebhook, Webhook,
};
pub(crate) use fluent_app::{FluentApp, TitleBar, WindowTitle};
pub(crate) use fluent_core::{AsyncResource, ResourceRequest, Theme, ThemeProvider as _};
pub(crate) use fluent_layout::{
    drag_source, drop_target, CommandPalette, ContextMenu, DockPanel, DockPosition, DocumentViewer,
    DropPolicy, ModalHost, PaletteEntry, Pane, PaneConstraints, PaneGroup, RichDocument,
    RichDocumentBlock, Workspace,
};
pub(crate) use fluent_primitives::{
    Button, ButtonAppearance, ButtonSize, Checkbox, CheckboxState, DateTimePicker, DateTimeValue,
    Dropdown, IconButton, ProgressBar, Searchbox, SuggestionItem, SuggestionKeyAction,
    SuggestionListState, TextInput, Textarea, TokenInput, TokenInputSuggestion, TokenInputToken,
};
pub(crate) use gpui::{
    actions, div, prelude::*, px, relative, rgb, svg, uniform_list, App, ClipboardItem, Context,
    Entity, FocusHandle, Focusable, FontWeight, Hsla, IntoElement, KeyBinding, KeyDownEvent,
    MouseButton, MouseDownEvent, Pixels, Point, Render, ScrollHandle, SharedString, Window,
};
pub(crate) use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
