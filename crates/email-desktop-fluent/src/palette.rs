use crate::prelude::*;
use crate::*;

pub(crate) fn palette_entries(
    state: &NativeAppState,
    state_entity: &Entity<NativeAppState>,
) -> Vec<PaletteEntry> {
    let mut entries = vec![
        PaletteEntry::new("compose", "New Message")
            .group("Actions")
            .hint("c")
            .subtitle("Compose a new email")
            .keyword("compose")
            .keyword("write")
            .disabled(state.accounts.is_empty())
            .action(|_, cx| cx.dispatch_action(&StartCompose)),
        PaletteEntry::new("advanced-search", "Advanced Search")
            .group("Actions")
            .subtitle("Build a structured mail search")
            .keyword("search")
            .keyword("filter")
            .action(|_, cx| cx.dispatch_action(&OpenAdvancedSearch)),
        PaletteEntry::new("settings", "Open Settings")
            .group("Actions")
            .subtitle("Open app and account settings")
            .keyword("preferences")
            .keyword("options")
            .action(|_, cx| cx.dispatch_action(&OpenSettings)),
        PaletteEntry::new("add-account", "Add Account")
            .group("Actions")
            .subtitle("Connect Gmail, Microsoft 365, or IMAP")
            .keyword("setup")
            .keyword("oauth")
            .keyword("imap")
            .action(|_, cx| cx.dispatch_action(&OpenAccountSetup)),
        PaletteEntry::new("theme-light", "Light theme")
            .group("Actions")
            .hint("Ctrl+Shift+L")
            .subtitle("Apply Fluent light theme")
            .keyword("appearance")
            .action(|_, cx| cx.dispatch_action(&ApplyLightTheme)),
        PaletteEntry::new("theme-dark", "Dark theme")
            .group("Actions")
            .hint("Ctrl+Shift+D")
            .subtitle("Apply Fluent dark theme")
            .keyword("appearance")
            .action(|_, cx| cx.dispatch_action(&ApplyDarkTheme)),
        PaletteEntry::new("density-down", "Density: Denser")
            .group("Actions")
            .hint("Ctrl+-")
            .keyword("compact")
            .keyword("layout")
            .disabled(state.density.level == 0)
            .action(|_, cx| cx.dispatch_action(&DecreaseDensity)),
        PaletteEntry::new("density-up", "Density: Spacious")
            .group("Actions")
            .hint("Ctrl++")
            .keyword("spacious")
            .keyword("layout")
            .disabled(state.density.level >= 8)
            .action(|_, cx| cx.dispatch_action(&IncreaseDensity)),
        PaletteEntry::new("view-mail", "Go to Mail")
            .group("Navigate")
            .subtitle("Open the mail workspace")
            .keyword("inbox")
            .action(|_, cx| cx.dispatch_action(&SelectMailView)),
        PaletteEntry::new("view-calendar", "Go to Calendar")
            .group("Navigate")
            .subtitle("Open the calendar workspace")
            .keyword("events")
            .keyword("schedule")
            .action(|_, cx| cx.dispatch_action(&SelectCalendarView)),
    ];

    for (kind, label) in [
        ("all", "All Inboxes"),
        ("unread", "Unread"),
        ("flagged", "Flagged"),
        ("snoozed", "Snoozed"),
    ] {
        let nav_state = state_entity.clone();
        let kind = kind.to_string();
        entries.push(
            PaletteEntry::new(format!("smart:{kind}"), label)
                .group("Navigate")
                .hint("smart")
                .subtitle("Open smart folder")
                .keyword("folder")
                .action(move |_, cx| {
                    start_message_collection_load(
                        &nav_state,
                        NativeMessageSource::SmartFolder(kind.clone()),
                        cx,
                    );
                }),
        );
    }

    if state.selected_message.is_some() {
        entries.push(
            PaletteEntry::new("message-reply", "Reply")
                .group("Message")
                .hint("r")
                .keyword("respond")
                .action(|_, cx| cx.dispatch_action(&StartReplyCompose)),
        );
        entries.push(
            PaletteEntry::new("message-forward", "Forward")
                .group("Message")
                .hint("f")
                .keyword("forward")
                .action(|_, cx| cx.dispatch_action(&StartForwardCompose)),
        );
    }

    if let Some((message_id, is_read, is_flagged)) = selected_palette_message_state(state) {
        let archive_state = state_entity.clone();
        let archive_id = message_id.clone();
        entries.push(
            PaletteEntry::new("message-archive", "Archive Message")
                .group("Message")
                .hint("e")
                .keyword("archive")
                .action(move |_, cx| {
                    start_message_mutation_and_refresh(
                        &archive_state,
                        NativeMessageMutation::Archive {
                            message_id: archive_id.clone(),
                        },
                        cx,
                    );
                }),
        );

        let delete_state = state_entity.clone();
        let delete_id = message_id.clone();
        entries.push(
            PaletteEntry::new("message-delete", "Delete Message")
                .group("Message")
                .hint("Del")
                .keyword("trash")
                .keyword("delete")
                .action(move |_, cx| {
                    start_message_mutation_and_refresh(
                        &delete_state,
                        NativeMessageMutation::Delete {
                            message_id: delete_id.clone(),
                        },
                        cx,
                    );
                }),
        );

        let read_state = state_entity.clone();
        let read_id = message_id.clone();
        entries.push(
            PaletteEntry::new(
                "message-toggle-read",
                if is_read { "Mark Unread" } else { "Mark Read" },
            )
            .group("Message")
            .hint("u")
            .keyword("read")
            .keyword("unread")
            .action(move |_, cx| {
                start_message_mutation_and_refresh(
                    &read_state,
                    NativeMessageMutation::SetRead {
                        message_id: read_id.clone(),
                        is_read: !is_read,
                    },
                    cx,
                );
            }),
        );

        let flag_state = state_entity.clone();
        let flag_id = message_id.clone();
        entries.push(
            PaletteEntry::new(
                "message-toggle-flag",
                if is_flagged {
                    "Unflag Message"
                } else {
                    "Flag Message"
                },
            )
            .group("Message")
            .keyword("flag")
            .keyword("star")
            .action(move |_, cx| {
                start_message_mutation_and_refresh(
                    &flag_state,
                    NativeMessageMutation::SetFlagged {
                        message_id: flag_id.clone(),
                        is_flagged: !is_flagged,
                    },
                    cx,
                );
            }),
        );
    }

    entries.extend(state.labels.iter().map(|label| {
        let nav_state = state_entity.clone();
        let label_id = label.id.clone();
        PaletteEntry::new(format!("label:{}", label.id), label.name.clone())
            .group("Labels")
            .hint("label")
            .subtitle("Open label")
            .keyword("label")
            .keyword("tag")
            .action(move |_, cx| {
                start_message_collection_load(
                    &nav_state,
                    NativeMessageSource::Label(label_id.clone()),
                    cx,
                );
            })
    }));

    entries.extend(state.folders_by_account.values().flat_map(|folders| {
        folders
            .iter()
            .filter(|folder| !folder.is_excluded)
            .map(|folder| {
                let nav_state = state_entity.clone();
                let folder_id = folder.id.clone();
                PaletteEntry::new(format!("folder:{}", folder.id), folder.name.clone())
                    .group("Folders")
                    .hint(folder.full_path.clone())
                    .subtitle("Open folder")
                    .keyword("folder")
                    .keyword(folder.full_path.clone())
                    .action(move |_, cx| {
                        start_message_collection_load(
                            &nav_state,
                            NativeMessageSource::Folder(folder_id.clone()),
                            cx,
                        );
                    })
            })
    }));

    entries.extend(state.accounts.iter().map(|account| {
        let nav_state = state_entity.clone();
        let folder_id = preferred_account_folder_id(state, &account.id);
        let mut entry = PaletteEntry::new(account.id.clone(), account.name.clone())
            .group("Accounts")
            .subtitle(account.email.clone())
            .keyword("account")
            .keyword("inbox")
            .disabled(folder_id.is_none());
        if let Some(folder_id) = folder_id {
            entry = entry.action(move |_, cx| {
                start_message_collection_load(
                    &nav_state,
                    NativeMessageSource::Folder(folder_id.clone()),
                    cx,
                );
            });
        }
        entry
    }));

    entries
}

pub(crate) fn selected_palette_message_state(
    state: &NativeAppState,
) -> Option<(String, bool, bool)> {
    let selected_id = state.selected_message.as_ref()?;
    if let Some(summary) = selected_summary(state) {
        return Some((summary.id.clone(), summary.is_read, summary.is_flagged));
    }

    state
        .selected_full_message
        .as_ref()
        .filter(|message| message.id == *selected_id)
        .map(|message| (message.id.clone(), message.is_read, message.is_flagged))
}

pub(crate) fn preferred_account_folder_id(
    state: &NativeAppState,
    account_id: &str,
) -> Option<String> {
    let folders = state.folders_by_account.get(account_id)?;
    folders
        .iter()
        .find(|folder| folder.special_use.as_deref() == Some("inbox") && !folder.is_excluded)
        .or_else(|| folders.iter().find(|folder| !folder.is_excluded))
        .map(|folder| folder.id.clone())
}
