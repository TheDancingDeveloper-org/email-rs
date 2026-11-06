use super::*;
use email_types::{MessageBody, RuleActionDto, RuleConditionDto};

fn account(id: &str) -> Account {
    Account {
        id: id.to_string(),
        name: "Account".to_string(),
        email: format!("{id}@example.test"),
        provider_type: "generic_imap".to_string(),
        auth_type: "basic".to_string(),
        created_at: "2026-05-25T00:00:00Z".to_string(),
        needs_reauth: false,
        reauth_reason: None,
    }
}

fn folder(id: &str, unread_count: i64) -> Folder {
    Folder {
        id: id.to_string(),
        account_id: "account".to_string(),
        name: id.to_string(),
        full_path: id.to_string(),
        special_use: None,
        unread_count,
        total_count: unread_count.max(0),
        synced_at: None,
        is_excluded: false,
    }
}

fn label(id: &str) -> Label {
    Label {
        id: id.to_string(),
        account_id: "account".to_string(),
        name: id.to_string(),
        color: "#5b5fc7".to_string(),
    }
}

fn message(id: &str, folder_id: &str, is_read: bool) -> Message {
    Message {
        id: id.to_string(),
        occurrence_id: id.to_string(),
        logical_id: format!("logical-{id}"),
        account_id: "account".to_string(),
        folder_id: folder_id.to_string(),
        uid: 1,
        message_id: None,
        thread_id: None,
        subject: Some("Subject".to_string()),
        from_name: None,
        from_email: None,
        to_json: None,
        date: None,
        is_read,
        is_flagged: false,
        is_draft: false,
        has_attachments: false,
        preview: None,
        snoozed_until: None,
    }
}

fn full_message(id: &str, folder_id: &str, is_read: bool, is_flagged: bool) -> FullMessage {
    FullMessage {
        id: id.to_string(),
        occurrence_id: id.to_string(),
        logical_id: format!("logical-{id}"),
        account_id: "account".to_string(),
        folder_id: folder_id.to_string(),
        uid: 1,
        message_id: None,
        thread_id: None,
        subject: Some("Subject".to_string()),
        from_name: None,
        from_email: None,
        to: None,
        date: None,
        is_read,
        is_flagged,
        is_draft: false,
        has_attachments: false,
        preview: None,
        body: None,
        attachments: Vec::new(),
        authentication_results: Default::default(),
        from_reply_to_domain_mismatch: false,
        display_name_spoofing: false,
    }
}

fn calendar_event(id: &str, day: u32) -> CalendarEvent {
    CalendarEvent {
        id: id.to_string(),
        calendar_id: "primary".to_string(),
        provider_event_id: None,
        title: "Planning".to_string(),
        description: None,
        start_at: Utc
            .with_ymd_and_hms(2026, 5, day, 14, 0, 0)
            .single()
            .expect("valid start"),
        end_at: Utc
            .with_ymd_and_hms(2026, 5, day, 15, 0, 0)
            .single()
            .expect("valid end"),
        location: None,
        is_all_day: false,
        recurrence_rule: None,
        attendees: Vec::new(),
        meet_link: None,
    }
}

fn local_calendar_event(
    id: &str,
    day: NaiveDate,
    start_hour: u32,
    start_minute: u32,
    end_day_offset: i64,
    end_hour: u32,
    end_minute: u32,
) -> CalendarEvent {
    let end_day = day + ChronoDuration::days(end_day_offset);
    CalendarEvent {
        id: id.to_string(),
        calendar_id: "primary".to_string(),
        provider_event_id: None,
        title: id.to_string(),
        description: None,
        start_at: Local
            .with_ymd_and_hms(
                day.year(),
                day.month(),
                day.day(),
                start_hour,
                start_minute,
                0,
            )
            .single()
            .expect("valid local start")
            .with_timezone(&Utc),
        end_at: Local
            .with_ymd_and_hms(
                end_day.year(),
                end_day.month(),
                end_day.day(),
                end_hour,
                end_minute,
                0,
            )
            .single()
            .expect("valid local end")
            .with_timezone(&Utc),
        location: None,
        is_all_day: false,
        recurrence_rule: None,
        attendees: Vec::new(),
        meet_link: None,
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {actual} to be close to {expected}"
    );
}

fn event_link(id: &str, event_id: &str, message_id: &str) -> EventLink {
    EventLink {
        id: id.to_string(),
        event_id: event_id.to_string(),
        message_id: message_id.to_string(),
        folder_id: "inbox".to_string(),
        subject: Some("Planning notes".to_string()),
        from_name: Some("Ada".to_string()),
        from_email: Some("ada@example.test".to_string()),
        date: Some("2026-05-26T12:00:00Z".to_string()),
        linked_at: "2026-05-26T13:00:00Z".to_string(),
    }
}

fn message_suggestion(id: &str, folder_id: &str) -> MessageSuggestion {
    MessageSuggestion {
        id: id.to_string(),
        folder_id: folder_id.to_string(),
        subject: Some("Planning notes".to_string()),
        from_name: Some("Ada".to_string()),
        from_email: Some("ada@example.test".to_string()),
        date: Some("2026-05-26T02:00:00Z".to_string()),
    }
}

fn contact(name: Option<&str>, email: &str) -> Contact {
    Contact {
        name: name.map(str::to_string),
        email: email.to_string(),
    }
}

fn calendar_suggestion(id: &str, start_at: &str) -> CalendarSuggestion {
    CalendarSuggestion {
        id: id.to_string(),
        title: "Planning".to_string(),
        start_at: start_at.to_string(),
    }
}

fn calendar_search_result(id: &str, start_at: &str) -> CalendarSearchResult {
    CalendarSearchResult {
        id: id.to_string(),
        title: "Planning".to_string(),
        description: None,
        start_at: start_at.to_string(),
        end_at: "2026-05-26T15:00:00Z".to_string(),
        location: Some("Room 1".to_string()),
        is_all_day: false,
    }
}

#[test]
fn native_window_title_uses_folder_unread_counts() {
    let mut state = NativeAppState::default();
    state.folders_by_account.insert(
        "account".to_string(),
        vec![folder("inbox", 2), folder("archive", 1), folder("bad", -4)],
    );

    assert_eq!(total_unread_count(&state), 3);
    assert_eq!(native_window_title(&state), "(3) email-rs FluentGUI");
    assert_eq!(native_window_status(&state), "3 unread messages");
}

#[test]
fn native_window_title_omits_zero_unread_prefix() {
    let state = NativeAppState::default();

    assert_eq!(native_window_title(&state), "email-rs FluentGUI");
    assert_eq!(native_window_status(&state), "No unread messages");
}

#[test]
fn native_app_assets_include_title_bar_icon() {
    let icon = gpui::AssetSource::load(&NativeFluentAssets, APP_ICON_PATH)
        .unwrap()
        .unwrap();
    let icon_text = std::str::from_utf8(&icon).unwrap();

    assert!(icon_text.contains("<svg"));
    assert!(icon_text.contains("#5b5fc7"));
    assert_eq!(
        gpui::AssetSource::list(&NativeFluentAssets, "icons").unwrap(),
        vec![SharedString::from("email-rs.svg")]
    );
}

#[test]
fn startup_loading_status_uses_account_loading_state() {
    let mut state = NativeAppState::default();
    let _ = state.accounts_resource.begin_load();

    assert_eq!(native_window_status(&state), "Loading accounts...");
    assert_eq!(
        message_list_status(&state),
        Some(("Loading accounts...".to_string(), false))
    );
}

#[test]
fn safe_attachment_filename_removes_path_and_control_characters() {
    assert_eq!(
        safe_attachment_filename("../bad/name:\n.pdf"),
        "_bad_name__.pdf"
    );
    assert_eq!(safe_attachment_filename("..."), "attachment");
    assert_eq!(safe_attachment_filename(" report.pdf "), "report.pdf");
}

#[test]
fn attachment_open_requires_confirmation_for_unsafe_content() {
    let mut attachment = MessageAttachment {
        id: "a".into(),
        message_id: "m".into(),
        filename: "invoice.pdf".into(),
        content_type: "application/pdf".into(),
        size_bytes: 1,
        dangerous_or_double_extension: false,
    };
    assert!(!attachment_requires_confirmation(&attachment));
    attachment.filename = "run.exe".into();
    attachment.content_type = "application/x-msdownload".into();
    assert!(attachment_requires_confirmation(&attachment));
    attachment.filename = "page.html".into();
    attachment.content_type = "text/html".into();
    assert!(attachment_requires_confirmation(&attachment));
}

#[cfg(unix)]
#[test]
fn saved_attachments_are_user_only() {
    use std::os::unix::fs::PermissionsExt as _;
    let path = std::env::temp_dir().join(format!(
        "email-private-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    write_private_attachment(&path, b"secret").unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn attachment_size_label_formats_common_units() {
    assert_eq!(attachment_size_label(-1), "0 B");
    assert_eq!(attachment_size_label(512), "512 B");
    assert_eq!(attachment_size_label(1536), "1.5 KB");
    assert_eq!(attachment_size_label(2 * 1024 * 1024), "2.0 MB");
}

#[test]
fn calendar_initial_scroll_targets_8am() {
    assert_eq!(CALENDAR_INITIAL_SCROLL_HOUR, 8);
}

#[test]
fn timed_event_layout_preserves_duration() {
    let day = NaiveDate::from_ymd_opt(2026, 5, 26).unwrap();
    let event = local_calendar_event("half-hour", day, 9, 0, 0, 9, 30);

    let layouts = timed_event_layouts_for_day(&[event], day);

    assert_eq!(layouts.len(), 1);
    assert_close(layouts[0].top, 9.0 * CALENDAR_HOUR_HEIGHT);
    assert_close(layouts[0].height, CALENDAR_HOUR_HEIGHT / 2.0);
    assert_eq!(layouts[0].column, 0);
    assert_eq!(layouts[0].column_count, 1);
}

#[test]
fn timed_event_layout_applies_minimum_height_for_short_events() {
    let day = NaiveDate::from_ymd_opt(2026, 5, 26).unwrap();
    let event = local_calendar_event("short", day, 9, 0, 0, 9, 5);

    let layouts = timed_event_layouts_for_day(&[event], day);

    assert_eq!(layouts.len(), 1);
    assert_close(layouts[0].height, MIN_TIMED_EVENT_HEIGHT);
}

#[test]
fn timed_event_layout_assigns_columns_for_overlaps() {
    let day = NaiveDate::from_ymd_opt(2026, 5, 26).unwrap();
    let first = local_calendar_event("first", day, 9, 0, 0, 10, 0);
    let second = local_calendar_event("second", day, 9, 30, 0, 10, 30);

    let layouts = timed_event_layouts_for_day(&[first, second], day);

    let first = layouts
        .iter()
        .find(|layout| layout.event.id == "first")
        .unwrap();
    let second = layouts
        .iter()
        .find(|layout| layout.event.id == "second")
        .unwrap();
    assert_eq!(first.column_count, 2);
    assert_eq!(second.column_count, 2);
    assert_ne!(first.column, second.column);
}

#[test]
fn timed_event_layout_reuses_column_for_back_to_back_events() {
    let day = NaiveDate::from_ymd_opt(2026, 5, 26).unwrap();
    let first = local_calendar_event("first", day, 9, 0, 0, 10, 0);
    let second = local_calendar_event("second", day, 10, 0, 0, 11, 0);

    let layouts = timed_event_layouts_for_day(&[first, second], day);

    assert_eq!(layouts.len(), 2);
    assert!(layouts
        .iter()
        .all(|layout| layout.column == 0 && layout.column_count == 1));
}

#[test]
fn timed_event_layout_clips_multi_day_events_to_each_day() {
    let day = NaiveDate::from_ymd_opt(2026, 5, 26).unwrap();
    let event = local_calendar_event("overnight", day, 23, 30, 1, 1, 0);

    let first_day = timed_event_layouts_for_day(std::slice::from_ref(&event), day);
    let second_day = timed_event_layouts_for_day(&[event], day + ChronoDuration::days(1));

    assert_eq!(first_day.len(), 1);
    assert_eq!(second_day.len(), 1);
    assert_close(first_day[0].top, 23.5 * CALENDAR_HOUR_HEIGHT);
    assert_close(first_day[0].height, CALENDAR_HOUR_HEIGHT / 2.0);
    assert_close(second_day[0].top, 0.0);
    assert_close(second_day[0].height, CALENDAR_HOUR_HEIGHT);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_headless_mode_respects_display_and_override_envs() {
    assert!(headless_mode_for_environment(true, true, true));
    assert!(!headless_mode_for_environment(false, true, false));
    assert!(!headless_mode_for_environment(false, false, true));
    assert!(headless_mode_for_environment(false, false, false));
}

#[cfg(not(target_os = "linux"))]
#[test]
fn non_linux_headless_mode_only_uses_override_env() {
    assert!(headless_mode_for_environment(true));
    assert!(!headless_mode_for_environment(false));
}

#[test]
fn native_key_bindings_include_ctrl_and_cmd_palette_shortcuts() {
    let specs = native_key_binding_specs();

    assert!(specs.contains(&("ctrl-k", NativeKeyAction::TogglePalette)));
    assert!(specs.contains(&("cmd-k", NativeKeyAction::TogglePalette)));
    assert_eq!(native_key_bindings().len(), specs.len());
}

#[test]
fn shortcut_registry_has_no_conflicts() {
    assert!(shortcut_conflicts().is_empty());
}

#[test]
fn shortcut_registry_covers_gmail_and_superhuman_actions() {
    let actions = SHORTCUT_COVERAGE
        .iter()
        .map(|(_, action, _)| *action)
        .collect::<BTreeSet<_>>();
    for required in [
        "Reply",
        "Archive",
        "Forward",
        "Compose",
        "Search",
        "Star",
        "Mark read",
        "Shortcut help",
        "Cycle pane",
    ] {
        assert!(
            actions.contains(required),
            "missing shortcut coverage for {required}"
        );
    }
}

#[test]
fn advanced_search_key_action_matches_web_modal_shortcuts() {
    assert_eq!(
        advanced_search_key_action("escape", false),
        Some(AdvancedSearchKeyAction::Close)
    );
    assert_eq!(
        advanced_search_key_action("enter", true),
        Some(AdvancedSearchKeyAction::Submit)
    );
    assert_eq!(
        advanced_search_key_action("return", true),
        Some(AdvancedSearchKeyAction::Submit)
    );
    assert_eq!(advanced_search_key_action("enter", false), None);
    assert_eq!(advanced_search_key_action("tab", true), None);
}

#[test]
fn native_preferences_parse_and_clamp_density() {
    let preferences = parse_native_preferences(
        "\n# comment\ntheme=light\ndensity=22\nemail_sidebar_w=900\nemail_msglist_w=99\nignored=value\n",
    );

    assert_eq!(
        preferences,
        NativePreferences {
            theme: ThemeMode::Light,
            density_level: 8,
            layout: NativeLayoutPreferences {
                sidebar_width: MAX_SIDEBAR_WIDTH,
                message_list_width: MIN_MESSAGE_LIST_WIDTH,
            },
            remote_image_allowlist: BTreeSet::new(),
            undo_window_secs: DEFAULT_UNDO_WINDOW_SECS,
        }
    );
}

#[test]
fn native_layout_preferences_translate_message_width_to_split_ratio() {
    let ratio = message_list_ratio_for_width(420, 1280.0, 280.0);
    assert!((ratio - 0.42).abs() < f32::EPSILON);
    assert_eq!(message_list_width_for_ratio(ratio, 1280.0, 280.0), 420);
}

#[test]
fn native_preferences_round_trip_to_disk() {
    let path = std::env::temp_dir().join(format!(
        "email-rs-native-prefs-{}-{}.conf",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let preferences = NativePreferences {
        theme: ThemeMode::Light,
        density_level: 2,
        layout: NativeLayoutPreferences {
            sidebar_width: 333,
            message_list_width: 444,
        },
        remote_image_allowlist: BTreeSet::from(["alice@example.test".to_string()]),
        undo_window_secs: 12,
    };

    write_native_preferences(&path, &preferences).expect("write preferences");
    let raw = std::fs::read_to_string(&path).expect("read preferences");
    assert_eq!(parse_native_preferences(&raw), preferences);

    let _ = std::fs::remove_file(path);
}

#[test]
fn remote_images_are_blocked_by_default_and_allowlist_is_case_insensitive() {
    let state = NativeAppState::default();
    assert!(!state.show_external_images);
    assert!(!sender_images_allowed(&state, "Alice@example.test"));

    let mut state = NativeAppState::default();
    state
        .remote_image_allowlist
        .insert("alice@example.test".to_string());
    let mut trusted = full_message("trusted", "inbox", true, false);
    trusted.authentication_results.dmarc = AuthenticationResultStatus::Pass;
    trusted.from_email = Some("alice@example.test".to_string());
    state.selected_full_message = Some(trusted);
    assert!(sender_images_allowed(&state, " Alice@EXAMPLE.TEST "));
    assert!(!sender_images_allowed(&state, "bob@example.test"));
}

#[test]
fn allowlist_is_applied_by_the_real_full_message_load_path() {
    let mut state = NativeAppState::default();
    state
        .remote_image_allowlist
        .insert("alice@example.test".to_string());
    let mut summary = message("trusted", "inbox", true);
    summary.from_email = Some("alice@example.test".to_string());
    state.messages = vec![summary];
    let mut loaded = full_message("trusted", "inbox", true, false);
    loaded.from_email = Some("alice@example.test".to_string());
    loaded.authentication_results.dmarc = AuthenticationResultStatus::Pass;

    begin_full_message_load(&mut state, "trusted").expect("request");
    assert!(!state.show_external_images);
    let request = state.selected_message_resource.current_request().unwrap();
    assert!(apply_loaded_full_message(
        &mut state,
        request,
        "trusted",
        LoadedFullMessage {
            message: loaded,
            labels: Vec::new(),
        },
    ));
    assert!(state.show_external_images);
}

#[test]
fn undo_pending_mutation_expires_and_is_retrieved_once() {
    let mutation = NativeMessageMutation::Archive {
        message_id: "one".into(),
    };
    let mut pending = Some(PendingUndo {
        mutation: mutation.clone(),
        expires_at: Instant::now() - Duration::from_secs(1),
    });
    assert_eq!(
        take_expired_pending_undo(&mut pending, Instant::now()),
        Some(mutation)
    );
    assert!(take_expired_pending_undo(&mut pending, Instant::now()).is_none());
}

#[test]
fn undo_before_expiry_keeps_mutation_reversible() {
    let mutation = NativeMessageMutation::Delete {
        message_id: "one".into(),
    };
    let mut pending = Some(PendingUndo {
        mutation: mutation.clone(),
        expires_at: Instant::now() + Duration::from_secs(5),
    });
    assert!(take_expired_pending_undo(&mut pending, Instant::now()).is_none());
    assert_eq!(pending.take().unwrap().mutation, mutation);
}

#[test]
fn replacing_undo_pending_mutation_returns_the_first_for_commit() {
    let first = NativeMessageMutation::Archive {
        message_id: "one".into(),
    };
    let second = NativeMessageMutation::Delete {
        message_id: "two".into(),
    };
    let mut pending = Some(PendingUndo {
        mutation: first.clone(),
        expires_at: Instant::now(),
    });
    assert_eq!(
        replace_pending_undo(
            &mut pending,
            PendingUndo {
                mutation: second.clone(),
                expires_at: Instant::now(),
            },
        ),
        Some(first)
    );
    assert_eq!(pending.unwrap().mutation, second);
}

#[test]
fn link_safety_warnings_cover_mismatches_and_idn_lookalikes() {
    let warnings = link_safety_warnings(
        r#"<p><a href="https://evil.example/login">https://example.com/login</a></p>
           <p><a href="https://xn--pple-43d.example/">https://apple.example/</a></p>
           <p><a href="https://аpple.example/">https://apple.example/</a></p>"#,
    );
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("names example.com")));
    assert!(warnings.iter().any(|warning| warning.contains("punycode")));
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("аpple.example")));

    assert!(link_safety_warnings(
        r#"<a href="https://www.example.com/path">https://example.com/path</a>"#
    )
    .is_empty());
}

#[test]
fn url_domain_parses_authority_not_path_or_userinfo() {
    assert_eq!(
        url_domain("https://evil.com/a@paypal.com"),
        Some("evil.com".into())
    );
    assert_eq!(
        url_domain("https://paypal.com@evil.com/login"),
        Some("evil.com".into())
    );
    assert_eq!(
        url_domain("https://paypal.com:443/login"),
        Some("paypal.com".into())
    );
    assert_eq!(
        url_domain("https://[2001:db8::1]:443/login"),
        Some("2001:db8::1".into())
    );
    assert_eq!(
        url_domain("https://xn--pple-43d.example/path"),
        Some("xn--pple-43d.example".into())
    );
    assert_eq!(url_domain("paypal.com"), Some("paypal.com".into()));
}

#[test]
fn external_images_are_stripped_from_render_document() {
    let mut document = RichDocument::from_html_fragment_lossy(
        r#"<p>hello</p><img src="https://tracker.example/pixel"><img src="cid:logo">"#,
    );
    strip_external_images(&mut document);
    assert!(document.blocks.iter().all(|block| {
        !matches!(block, RichDocumentBlock::Image { src, .. } if src.starts_with("http://") || src.starts_with("https://"))
    }));
    assert!(document
        .blocks
        .iter()
        .any(|block| matches!(block, RichDocumentBlock::Image { src, .. } if src == "cid:logo")));
}

#[test]
fn sender_trust_status_and_mismatch_warnings_use_server_signals() {
    let mut state = NativeAppState::default();
    let mut message = full_message("message", "inbox", true, false);
    message.authentication_results.spf = AuthenticationResultStatus::Pass;
    message.authentication_results.dkim = AuthenticationResultStatus::Fail;
    message.from_reply_to_domain_mismatch = true;
    state.selected_full_message = Some(message);

    assert_eq!(
        selected_trust_status(&state).as_deref(),
        Some("Sender authentication: SPF pass, DKIM failed, DMARC not reported")
    );
    assert_eq!(selected_sender_warnings(&state).len(), 1);
}

#[test]
fn native_message_source_uses_current_selection() {
    let mut state = NativeAppState {
        selected_folder: Some("smart:unread".to_string()),
        ..NativeAppState::default()
    };
    assert_eq!(
        NativeMessageSource::from_state(&state),
        Some(NativeMessageSource::SmartFolder("unread".to_string()))
    );

    state.selected_label = Some("label-a".to_string());
    assert_eq!(
        NativeMessageSource::from_state(&state),
        Some(NativeMessageSource::Label("label-a".to_string()))
    );

    state.selected_label = None;
    state.selected_folder = Some("inbox".to_string());
    assert_eq!(
        NativeMessageSource::from_state(&state),
        Some(NativeMessageSource::Folder("inbox".to_string()))
    );
}

#[test]
fn search_source_records_previous_route_and_updates_title() {
    let mut state = NativeAppState {
        selected_folder: Some("inbox".to_string()),
        ..NativeAppState::default()
    };

    NativeMessageSource::Search("quarterly report".to_string()).apply_selection(&mut state);

    assert_eq!(state.selected_folder, None);
    assert_eq!(state.selected_label, None);
    assert_eq!(state.search.query, "quarterly report");
    assert_eq!(
        state.search.submitted_query.as_deref(),
        Some("quarterly report")
    );
    assert_eq!(
        state.search.previous_source,
        Some(NativeMessageSource::Folder("inbox".to_string()))
    );
    assert_eq!(
        NativeMessageSource::from_state(&state),
        Some(NativeMessageSource::Search("quarterly report".to_string()))
    );
    assert_eq!(
        current_mailbox_title(&state),
        "Results for \"quarterly report\""
    );
}

#[test]
fn advanced_search_source_records_previous_route_and_updates_title() {
    let group = ConditionGroup {
        match_mode: MatchMode::Any,
        conditions: vec![Condition {
            id: "c1".to_string(),
            field: ConditionField::Subject,
            operator: ConditionOperator::StartsWith,
            value: "quarterly".to_string(),
        }],
    };
    let mut state = NativeAppState {
        selected_folder: Some("inbox".to_string()),
        ..NativeAppState::default()
    };

    NativeMessageSource::AdvancedSearch(group.clone()).apply_selection(&mut state);

    assert_eq!(state.selected_folder, None);
    assert_eq!(state.selected_label, None);
    assert_eq!(state.search.query, "");
    assert_eq!(state.search.submitted_query, None);
    assert_eq!(state.search.condition_group, Some(group.clone()));
    assert_eq!(
        state.search.previous_source,
        Some(NativeMessageSource::Folder("inbox".to_string()))
    );
    assert_eq!(
        NativeMessageSource::from_state(&state),
        Some(NativeMessageSource::AdvancedSearch(group))
    );
    assert_eq!(current_mailbox_title(&state), "Advanced search results");
}

#[test]
fn basic_search_clears_advanced_search_state() {
    let mut state = NativeAppState {
        search: SearchState {
            condition_group: Some(ConditionGroup {
                match_mode: MatchMode::All,
                conditions: vec![Condition {
                    id: "c1".to_string(),
                    field: ConditionField::From,
                    operator: ConditionOperator::Contains,
                    value: "ada".to_string(),
                }],
            }),
            ..SearchState::default()
        },
        ..NativeAppState::default()
    };

    NativeMessageSource::Search("status".to_string()).apply_selection(&mut state);

    assert_eq!(state.search.condition_group, None);
    assert_eq!(state.search.submitted_query.as_deref(), Some("status"));
    assert_eq!(current_mailbox_title(&state), "Results for \"status\"");
}

#[test]
fn debounced_search_waits_for_due_time_and_submits_latest_query() {
    let start = Instant::now();
    let mut search = SearchState {
        query: " plan ".to_string(),
        ..SearchState::default()
    };

    schedule_debounced_search(&mut search, " plan ", start);

    assert_eq!(
        search.pending_query,
        Some(DebouncedSearch {
            query: "plan".to_string(),
            due_at: start + SEARCH_RESULTS_DEBOUNCE,
        })
    );
    assert_eq!(
        take_due_debounced_search(
            &mut search,
            start + SEARCH_RESULTS_DEBOUNCE - Duration::from_millis(1),
        ),
        None
    );
    assert!(search.pending_query.is_some());
    assert_eq!(
        take_due_debounced_search(&mut search, start + SEARCH_RESULTS_DEBOUNCE),
        Some(NativeMessageSource::Search("plan".to_string()))
    );
    assert!(search.pending_query.is_none());
}

#[test]
fn debounced_search_skips_stale_empty_and_already_submitted_queries() {
    let start = Instant::now();
    let mut search = SearchState {
        query: "plan".to_string(),
        ..SearchState::default()
    };

    schedule_debounced_search(&mut search, "plan", start);
    search.query = "other".to_string();
    assert_eq!(
        take_due_debounced_search(&mut search, start + SEARCH_RESULTS_DEBOUNCE),
        None
    );
    assert!(search.pending_query.is_none());

    search.query = "other".to_string();
    search.submitted_query = Some("other".to_string());
    schedule_debounced_search(&mut search, "other", start);
    assert_eq!(
        take_due_debounced_search(&mut search, start + SEARCH_RESULTS_DEBOUNCE),
        None
    );

    schedule_debounced_search(&mut search, " ", start);
    assert!(search.pending_query.is_none());
}

#[test]
fn sidebar_search_suggestion_items_format_message_and_event_metadata() {
    let suggestions = vec![
        NativeSearchSuggestion::Message(message_suggestion("message", "inbox")),
        NativeSearchSuggestion::Event(calendar_suggestion("event", "2026-05-26T02:00:00Z")),
    ];

    let items = sidebar_search_suggestion_items(&suggestions);

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id.as_ref(), "message:message");
    assert_eq!(items[0].label.as_ref(), "Planning notes");
    assert_eq!(
        items[0].detail.as_ref().map(|detail| detail.to_string()),
        Some("Ada".to_string())
    );
    assert_eq!(items[1].id.as_ref(), "event:event");
    assert_eq!(items[1].label.as_ref(), "Planning");
    assert_eq!(
        items[1].detail.as_ref().map(|detail| detail.to_string()),
        Some("Calendar event".to_string())
    );
    assert!(items[1].meta.is_some());
}

#[test]
fn applying_sidebar_search_suggestions_tracks_query_and_stale_requests() {
    let mut state = NativeAppState::default();
    state.search.query = "plan".to_string();
    state.search.suggestions_query = Some("plan".to_string());
    let stale = state.search.suggestions_resource.begin_load();
    let current = state.search.suggestions_resource.begin_load();

    assert!(!apply_loaded_sidebar_search_suggestions(
        &mut state,
        stale,
        "plan",
        vec![NativeSearchSuggestion::Message(message_suggestion(
            "old", "inbox"
        ))],
    ));
    assert!(apply_loaded_sidebar_search_suggestions(
        &mut state,
        current,
        "plan",
        vec![
            NativeSearchSuggestion::Message(message_suggestion("message", "inbox")),
            NativeSearchSuggestion::Event(calendar_suggestion("event", "2026-05-26T02:00:00Z")),
        ],
    ));
    assert!(state.search.suggestions_open);
    assert_eq!(state.search.suggestions.len(), 2);

    let request = state.search.suggestions_resource.begin_load();
    state.search.query = "other".to_string();
    assert!(!apply_loaded_sidebar_search_suggestions(
        &mut state,
        request,
        "plan",
        vec![NativeSearchSuggestion::Message(message_suggestion(
            "late", "inbox"
        ))],
    ));
    assert_eq!(state.search.suggestions.len(), 2);
}

#[test]
fn sidebar_search_suggestion_keyboard_state_moves_and_clears() {
    let mut search = SearchState {
        suggestions_open: true,
        suggestions: vec![
            NativeSearchSuggestion::Message(message_suggestion("message", "inbox")),
            NativeSearchSuggestion::Event(calendar_suggestion("event", "2026-05-26T02:00:00Z")),
        ],
        suggestions_resource: AsyncResource::ready(Vec::new()),
        ..SearchState::default()
    };
    let items = sidebar_search_suggestion_items(&search.suggestions);

    assert_eq!(
        search.suggestion_list.handle_key("down", &items),
        SuggestionKeyAction::Move(Some(0))
    );
    assert_eq!(
        search.suggestion_list.handle_key("down", &items),
        SuggestionKeyAction::Move(Some(1))
    );
    assert_eq!(
        search.suggestion_list.handle_key("enter", &items),
        SuggestionKeyAction::Select("event:event".into())
    );
    assert_eq!(
        highlighted_sidebar_search_suggestion_id(&search).as_deref(),
        Some("event:event")
    );

    clear_sidebar_search_suggestions(&mut search);
    assert!(!search.suggestions_open);
    assert!(search.suggestions.is_empty());
    assert!(search.suggestions_resource.is_idle());
    assert_eq!(search.suggestion_list.highlighted(), None);
    assert_eq!(highlighted_sidebar_search_suggestion_id(&search), None);
}

#[test]
fn route_selection_clears_active_search() {
    let mut state = NativeAppState {
        search: SearchState {
            query: "report".to_string(),
            submitted_query: Some("report".to_string()),
            condition_group: None,
            previous_source: Some(NativeMessageSource::Folder("inbox".to_string())),
            ..SearchState::default()
        },
        ..NativeAppState::default()
    };

    NativeMessageSource::SmartFolder("unread".to_string()).apply_selection(&mut state);

    assert_eq!(state.search, SearchState::default());
    assert_eq!(state.selected_folder.as_deref(), Some("smart:unread"));
    assert_eq!(state.selected_label, None);
}

#[test]
fn selected_regular_folder_id_skips_search_label_and_smart_views() {
    let mut state = NativeAppState {
        selected_folder: Some("inbox".to_string()),
        ..NativeAppState::default()
    };
    assert_eq!(selected_regular_folder_id(&state), Some("inbox"));

    state.selected_folder = Some("smart:unread".to_string());
    assert_eq!(selected_regular_folder_id(&state), None);

    state.selected_folder = Some("inbox".to_string());
    state.selected_label = Some("label".to_string());
    assert_eq!(selected_regular_folder_id(&state), None);

    state.selected_label = None;
    state.search.submitted_query = Some("report".to_string());
    assert_eq!(selected_regular_folder_id(&state), None);
}

#[test]
fn message_list_render_rows_group_regular_folder_threads() {
    let mut older = message("older", "inbox", true);
    older.thread_id = Some("thread-a".to_string());
    older.date = Some("2026-05-25T12:00:00Z".to_string());
    let mut latest = message("latest", "inbox", false);
    latest.thread_id = Some("thread-a".to_string());
    latest.date = Some("2026-05-26T12:00:00Z".to_string());
    let mut solo = message("solo", "inbox", true);
    solo.date = Some("2026-05-26T10:00:00Z".to_string());
    let mut state = NativeAppState {
        selected_folder: Some("inbox".to_string()),
        messages: vec![older, solo, latest],
        ..NativeAppState::default()
    };

    let rows = message_list_render_rows(&state);
    assert_eq!(
        rows.iter()
            .filter_map(MessageListRenderRow::message_id)
            .collect::<Vec<_>>(),
        vec!["latest", "solo"]
    );
    assert_eq!(rows[0].thread_count(), Some(2));
    assert!(!rows[0].is_indented());

    state.expanded_thread_keys.insert("thread-a".to_string());
    let rows = message_list_render_rows(&state);
    assert_eq!(
        rows.iter()
            .filter_map(MessageListRenderRow::message_id)
            .collect::<Vec<_>>(),
        vec!["latest", "older", "solo"]
    );
    assert!(rows[1].is_indented());
    assert!(matches!(
        &rows[2],
        MessageListRenderRow::CollapseThread { thread_key }
            if thread_key == "thread-a"
    ));
}

#[test]
fn message_list_render_rows_keep_non_regular_views_flat() {
    let mut first = message("first", "inbox", true);
    first.thread_id = Some("thread-a".to_string());
    let mut second = message("second", "inbox", true);
    second.thread_id = Some("thread-a".to_string());
    let mut state = NativeAppState {
        selected_folder: Some("inbox".to_string()),
        messages: vec![first, second],
        ..NativeAppState::default()
    };

    state.search.submitted_query = Some("status".to_string());
    let rows = message_list_render_rows(&state);
    assert_eq!(
        rows.iter()
            .filter_map(MessageListRenderRow::message_id)
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );

    state.search = SearchState::default();
    state.selected_folder = Some("smart:unread".to_string());
    let rows = message_list_render_rows(&state);
    assert_eq!(
        rows.iter()
            .filter_map(MessageListRenderRow::message_id)
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );

    state.selected_folder = None;
    state.selected_label = Some("label".to_string());
    let rows = message_list_render_rows(&state);
    assert_eq!(
        rows.iter()
            .filter_map(MessageListRenderRow::message_id)
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
}

#[test]
fn preferred_account_folder_prefers_visible_inbox() {
    let mut archive = folder("archive", 0);
    let mut inbox = folder("inbox", 0);
    inbox.special_use = Some("inbox".to_string());
    let mut state = NativeAppState {
        folders_by_account: BTreeMap::from([(
            "account".to_string(),
            vec![archive.clone(), inbox.clone()],
        )]),
        ..NativeAppState::default()
    };

    assert_eq!(
        preferred_account_folder_id(&state, "account").as_deref(),
        Some("inbox")
    );

    archive.is_excluded = false;
    inbox.is_excluded = true;
    state
        .folders_by_account
        .insert("account".to_string(), vec![archive, inbox]);

    assert_eq!(
        preferred_account_folder_id(&state, "account").as_deref(),
        Some("archive")
    );
}

#[test]
fn selected_palette_message_state_uses_summary_then_full_message() {
    let mut summary = message("message", "inbox", false);
    summary.is_flagged = true;
    let mut state = NativeAppState {
        selected_message: Some("message".to_string()),
        messages: vec![summary],
        selected_full_message: Some(full_message("message", "inbox", true, false)),
        ..NativeAppState::default()
    };

    assert_eq!(
        selected_palette_message_state(&state),
        Some(("message".to_string(), false, true))
    );

    state.messages.clear();
    assert_eq!(
        selected_palette_message_state(&state),
        Some(("message".to_string(), true, false))
    );
}

#[test]
fn message_key_target_tracks_roving_selection() {
    let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];

    assert_eq!(
        message_key_target(&ids, Some("b"), "down").as_deref(),
        Some("c")
    );
    assert_eq!(
        message_key_target(&ids, Some("b"), "up").as_deref(),
        Some("a")
    );
    assert_eq!(
        message_key_target(&ids, Some("b"), "j").as_deref(),
        Some("c")
    );
    assert_eq!(
        message_key_target(&ids, Some("b"), "k").as_deref(),
        Some("a")
    );
    assert_eq!(
        message_key_target(&ids, Some("b"), "return").as_deref(),
        Some("b")
    );
    assert_eq!(
        message_key_target(&ids, Some("z"), "down").as_deref(),
        Some("b")
    );
    assert_eq!(message_key_target(&ids, Some("b"), "x"), None);
}

#[test]
fn reading_pane_key_action_maps_web_shortcuts() {
    assert_eq!(
        reading_pane_key_action("e", false, false),
        Some(ReadingPaneKeyAction::Archive)
    );
    assert_eq!(
        reading_pane_key_action("#", false, false),
        Some(ReadingPaneKeyAction::Delete)
    );
    assert_eq!(
        reading_pane_key_action("3", true, false),
        Some(ReadingPaneKeyAction::Delete)
    );
    assert_eq!(
        reading_pane_key_action("u", false, false),
        Some(ReadingPaneKeyAction::ToggleRead)
    );
    assert_eq!(
        reading_pane_key_action("escape", false, false),
        Some(ReadingPaneKeyAction::ClearSelection)
    );
    assert_eq!(reading_pane_key_action("e", false, true), None);
    assert_eq!(reading_pane_key_action("r", false, false), None);
    assert_eq!(
        reading_pane_key_action("c", false, true),
        Some(ReadingPaneKeyAction::CopyBody)
    );
}

#[test]
fn snooze_presets_match_web_offsets() {
    let now = Local
        .with_ymd_and_hms(2026, 5, 26, 14, 45, 0)
        .single()
        .expect("valid local time");

    let later_today = snooze_preset_value_at(SnoozePreset::LaterToday, now);
    assert_eq!(
        (
            later_today.year,
            later_today.month,
            later_today.day,
            later_today.hour,
            later_today.minute,
        ),
        (2026, 5, 26, 17, 0)
    );

    let tomorrow = snooze_preset_value_at(SnoozePreset::TomorrowMorning, now);
    assert_eq!(
        (
            tomorrow.year,
            tomorrow.month,
            tomorrow.day,
            tomorrow.hour,
            tomorrow.minute,
        ),
        (2026, 5, 27, 9, 0)
    );

    let next_monday = snooze_preset_value_at(SnoozePreset::NextMonday, now);
    assert_eq!(
        (
            next_monday.year,
            next_monday.month,
            next_monday.day,
            next_monday.hour,
            next_monday.minute,
        ),
        (2026, 6, 1, 9, 0)
    );
}

#[test]
fn next_monday_snooze_skips_current_monday() {
    let monday = Local
        .with_ymd_and_hms(2026, 6, 1, 8, 30, 0)
        .single()
        .expect("valid local time");
    let sunday = Local
        .with_ymd_and_hms(2026, 6, 7, 8, 30, 0)
        .single()
        .expect("valid local time");

    let from_monday = snooze_preset_value_at(SnoozePreset::NextMonday, monday);
    assert_eq!(
        (
            from_monday.year,
            from_monday.month,
            from_monday.day,
            from_monday.hour,
            from_monday.minute,
        ),
        (2026, 6, 8, 9, 0)
    );

    let from_sunday = snooze_preset_value_at(SnoozePreset::NextMonday, sunday);
    assert_eq!(
        (
            from_sunday.year,
            from_sunday.month,
            from_sunday.day,
            from_sunday.hour,
            from_sunday.minute,
        ),
        (2026, 6, 8, 9, 0)
    );
}

#[test]
fn message_selection_helpers_track_visible_rows() {
    let mut state = NativeAppState {
        messages: vec![
            message("a", "inbox", true),
            message("b", "inbox", false),
            message("c", "inbox", true),
        ],
        ..NativeAppState::default()
    };

    assert_eq!(message_select_all_state(&state), CheckboxState::Unchecked);
    toggle_message_selection(&mut state, "b");
    assert_eq!(state.selected_message_ids, vec!["b"]);
    assert_eq!(
        selected_action_message_ids(&state, Some("a")),
        vec!["b".to_string()]
    );
    assert_eq!(
        message_select_all_state(&state),
        CheckboxState::Indeterminate
    );

    set_all_message_selection(&mut state, true);
    assert_eq!(state.selected_message_ids, vec!["a", "b", "c"]);
    assert_eq!(message_select_all_state(&state), CheckboxState::Checked);

    state.messages.remove(1);
    retain_existing_message_selection(&mut state);
    assert_eq!(state.selected_message_ids, vec!["a", "c"]);

    set_all_message_selection(&mut state, false);
    assert!(state.selected_message_ids.is_empty());
}

#[test]
fn sender_initials_use_display_name_or_fallback() {
    assert_eq!(sender_initials("Ada Lovelace"), "AL");
    assert_eq!(sender_initials("alerts@example.test"), "A");
    assert_eq!(sender_initials(""), "U");
}

#[test]
fn label_color_parser_accepts_css_hex_colors() {
    assert!(parse_label_color("#5b5fc7").is_some());
    assert_eq!(parse_label_color("#abc"), parse_label_color("#aabbcc"));
    assert_eq!(parse_label_color("5B5FC7"), parse_label_color("#5b5fc7"));
    assert!(parse_label_color("not-a-color").is_none());
    assert!(parse_label_color("#12").is_none());
}

#[test]
fn recipient_suggestions_prefer_contacts_and_dedupe_visible_senders() {
    let mut ada_message = message("ada-message", "inbox", true);
    ada_message.from_name = Some("Visible Ada".to_string());
    ada_message.from_email = Some("ada@example.test".to_string());
    let mut bob_message = message("bob-message", "inbox", true);
    bob_message.from_name = Some("Bob Sender".to_string());
    bob_message.from_email = Some("bob@example.test".to_string());
    let state = NativeAppState {
        messages: vec![ada_message, bob_message],
        compose: ComposeState {
            recipient_suggestions: vec![
                contact(Some("Ada Contact"), "ada@example.test"),
                contact(None, "ops@example.test"),
            ],
            ..ComposeState::default()
        },
        ..NativeAppState::default()
    };

    let suggestions = recipient_suggestions(&state);
    let values = suggestions
        .iter()
        .map(|suggestion| suggestion.value.to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        vec!["ada@example.test", "ops@example.test", "bob@example.test"]
    );
    assert_eq!(suggestions[0].label.to_string(), "Ada Contact");
    assert_eq!(
        suggestions[0].detail.as_ref().map(ToString::to_string),
        Some("ada@example.test".to_string())
    );
    assert_eq!(suggestions[1].label.to_string(), "ops@example.test");
}

#[test]
fn applying_recipient_suggestions_rejects_stale_queries() {
    let mut state = NativeAppState::default();
    state.compose.recipient_suggestions_query = Some("ada".to_string());
    let stale = state.compose.recipient_suggestions_resource.begin_load();
    let current = state.compose.recipient_suggestions_resource.begin_load();

    assert!(!apply_loaded_recipient_suggestions(
        &mut state,
        stale,
        "ada",
        vec![contact(Some("Old Ada"), "old@example.test")]
    ));
    assert!(state.compose.recipient_suggestions.is_empty());

    assert!(apply_loaded_recipient_suggestions(
        &mut state,
        current,
        "ada",
        vec![contact(Some("Ada"), "ada@example.test")]
    ));
    assert_eq!(state.compose.recipient_suggestions.len(), 1);

    let request = state.compose.recipient_suggestions_resource.begin_load();
    state.compose.recipient_suggestions_query = Some("bob".to_string());
    assert!(!reject_recipient_suggestions_load(
        &mut state,
        request,
        "ada",
        anyhow!("late failure")
    ));
    assert_eq!(
        state.compose.recipient_suggestions[0].email,
        "ada@example.test"
    );

    clear_recipient_suggestions(&mut state.compose);
    assert!(state.compose.recipient_suggestions_query.is_none());
    assert!(state.compose.recipient_suggestions.is_empty());
    assert!(state.compose.recipient_suggestions_resource.is_idle());
}

#[test]
fn calendar_week_helpers_use_monday_start_and_week_range() {
    let sunday = NaiveDate::from_ymd_opt(2026, 5, 31).expect("date");
    let monday = NaiveDate::from_ymd_opt(2026, 5, 25).expect("date");
    assert_eq!(monday_of_date(sunday), monday);
    assert_eq!(calendar_week_days(monday).len(), 7);
    assert_eq!(calendar_week_range_text(monday), "May 25 - 31, 2026");

    let year_end = NaiveDate::from_ymd_opt(2026, 12, 28).expect("date");
    assert_eq!(
        calendar_week_range_text(year_end),
        "Dec 28, 2026 - Jan 3, 2027"
    );
}

#[test]
fn calendar_search_result_selection_targets_event_week() {
    let mut state = NativeAppState {
        selected_view: SelectedView::Mail,
        selected_message_ids: vec!["message".to_string()],
        calendar_event_links_event: Some("old-event".to_string()),
        calendar_event_links: vec![event_link("old-link", "old-event", "message")],
        calendar_event_links_resource: AsyncResource::ready(vec![event_link(
            "old-link",
            "old-event",
            "message",
        )]),
        ..NativeAppState::default()
    };

    apply_calendar_search_result_selection(&mut state, "event".to_string(), "2026-05-31T23:30:00Z")
        .expect("valid event start");

    assert_eq!(state.selected_view, SelectedView::Calendar);
    assert_eq!(
        state.calendar_week_start,
        calendar_week_start_from_iso("2026-05-31T23:30:00Z").expect("week")
    );
    assert_eq!(state.selected_event.as_deref(), Some("event"));
    assert!(state.selected_message_ids.is_empty());
    assert!(state.calendar_event_links.is_empty());
    assert!(state.calendar_event_links_resource.is_idle());
}

#[test]
fn calendar_search_result_labels_format_time_and_fallback() {
    let event = calendar_search_result("event", "2026-05-26T02:00:00Z");
    assert!(calendar_search_result_time_label(&event).contains("May 26"));
    assert!(calendar_search_result_time_label(&event).contains("M"));

    let mut invalid = event.clone();
    invalid.start_at = "not-a-date".to_string();
    assert_eq!(calendar_search_result_time_label(&invalid), "not-a-date");
    assert!(apply_calendar_search_result_selection(
        &mut NativeAppState::default(),
        "event".to_string(),
        "not-a-date"
    )
    .is_err());
}

#[test]
fn applying_calendar_events_tracks_selection_and_stale_requests() {
    let mut state = NativeAppState {
        selected_event: Some("keep".to_string()),
        ..NativeAppState::default()
    };
    let stale = state.calendar_events_resource.begin_load();
    let current = state.calendar_events_resource.begin_load();

    assert!(!apply_loaded_calendar_events(
        &mut state,
        stale,
        vec![calendar_event("stale", 26)]
    ));
    assert!(apply_loaded_calendar_events(
        &mut state,
        current,
        vec![calendar_event("keep", 26)]
    ));
    assert_eq!(state.selected_event.as_deref(), Some("keep"));
    assert_eq!(state.calendar_events.len(), 1);

    let request = state.calendar_events_resource.begin_load();
    assert!(apply_loaded_calendar_events(
        &mut state,
        request,
        vec![calendar_event("other", 27)]
    ));
    assert_eq!(state.selected_event, None);
    assert!(state.calendar_event_links.is_empty());
    assert!(state.calendar_event_links_resource.is_idle());
}

#[test]
fn applying_calendar_event_links_tracks_selected_event_and_stale_requests() {
    let mut state = NativeAppState {
        selected_event: Some("event-a".to_string()),
        calendar_event_links_event: Some("event-a".to_string()),
        ..NativeAppState::default()
    };
    let stale = state.calendar_event_links_resource.begin_load();
    let current = state.calendar_event_links_resource.begin_load();

    assert!(!apply_loaded_calendar_event_links(
        &mut state,
        stale,
        "event-a",
        vec![event_link("stale", "event-a", "old-message")]
    ));
    assert!(apply_loaded_calendar_event_links(
        &mut state,
        current,
        "event-a",
        vec![event_link("link-a", "event-a", "message-a")]
    ));
    assert_eq!(state.calendar_event_links.len(), 1);
    assert_eq!(state.calendar_event_links[0].message_id, "message-a");

    let request = state.calendar_event_links_resource.begin_load();
    state.selected_event = Some("event-b".to_string());
    state.calendar_event_links_event = Some("event-b".to_string());
    assert!(!apply_loaded_calendar_event_links(
        &mut state,
        request,
        "event-a",
        vec![event_link("link-b", "event-a", "message-b")]
    ));
    assert_eq!(state.calendar_event_links[0].message_id, "message-a");
}

#[test]
fn calendar_link_labels_use_subject_sender_and_date_fallbacks() {
    let mut link = event_link("link", "event", "message");
    assert_eq!(calendar_link_subject(&link), "Planning notes");
    assert_eq!(calendar_link_sender(&link), "Ada");
    assert_eq!(calendar_link_meta(&link), "Ada - May 26, 2026");

    link.subject = Some("   ".to_string());
    link.from_name = Some("   ".to_string());
    link.from_email = Some("fallback@example.test".to_string());
    link.date = Some("not-a-date".to_string());
    assert_eq!(calendar_link_subject(&link), "(no subject)");
    assert_eq!(calendar_link_sender(&link), "fallback@example.test");
    assert_eq!(
        calendar_link_meta(&link),
        "fallback@example.test - not-a-date"
    );
}

#[test]
fn selected_sender_email_prefers_full_message_then_summary() {
    let mut summary = message("message", "inbox", true);
    summary.from_email = Some("summary@example.test".to_string());
    let mut full = full_message("message", "inbox", true, false);
    full.from_email = Some(" full@example.test ".to_string());
    let mut state = NativeAppState {
        selected_message: Some("message".to_string()),
        messages: vec![summary],
        selected_full_message: Some(full),
        ..NativeAppState::default()
    };

    assert_eq!(
        selected_sender_email(&state).as_deref(),
        Some("full@example.test")
    );

    state.selected_full_message = None;
    assert_eq!(
        selected_sender_email(&state).as_deref(),
        Some("summary@example.test")
    );
}

#[test]
fn selected_document_prefers_safe_html_body() {
    let mut full = full_message("message", "inbox", true, false);
    full.body = Some(MessageBody {
        html_body: Some(
            "<h1>Hello</h1><style>.x{}</style><script>alert(1)</script><p>Body &amp; more</p>"
                .to_string(),
        ),
        text_body: Some("Text fallback".to_string()),
        ..Default::default()
    });
    let state = NativeAppState {
        selected_full_message: Some(full),
        ..NativeAppState::default()
    };

    let text = selected_document(&state).to_plain_text();
    assert!(text.contains("Hello"));
    assert!(text.contains("Body & more"));
    assert!(!text.contains("alert"));
    assert!(!text.contains("Text fallback"));
}

#[test]
fn selected_document_strips_external_images_unless_shown() {
    let mut full = full_message("message", "inbox", true, false);
    full.body = Some(MessageBody {
        html_body: Some(
            r#"<p>hi</p><img src="https://tracker.example/p.png"><img src="cid:logo">"#.to_string(),
        ),
        ..Default::default()
    });
    let mut state = NativeAppState {
        selected_full_message: Some(full),
        show_external_images: false,
        ..NativeAppState::default()
    };
    let remote = |doc: &RichDocument| {
        doc.blocks.iter().any(|block| {
            matches!(block, RichDocumentBlock::Image { src, .. } if src.starts_with("https://"))
        })
    };

    assert!(!remote(&selected_document(&state)));
    state.show_external_images = true;
    assert!(remote(&selected_document(&state)));
}

#[test]
fn selected_document_falls_back_to_text_and_preview() {
    let mut full = full_message("message", "inbox", true, false);
    full.preview = Some("Preview fallback".to_string());
    full.body = Some(MessageBody {
        html_body: Some("   ".to_string()),
        text_body: Some("Text body".to_string()),
        ..Default::default()
    });
    let mut state = NativeAppState {
        selected_full_message: Some(full),
        ..NativeAppState::default()
    };

    assert_eq!(selected_document(&state).to_plain_text(), "Text body");

    let mut full = full_message("message", "inbox", true, false);
    full.preview = Some("Preview fallback".to_string());
    state.selected_full_message = Some(full);
    assert_eq!(
        selected_document(&state).to_plain_text(),
        "Preview fallback"
    );

    state.selected_full_message = None;
    state.selected_message = Some("summary".to_string());
    let mut summary = message("summary", "inbox", true);
    summary.preview = Some("Summary preview".to_string());
    state.messages = vec![summary];
    assert_eq!(selected_document(&state).to_plain_text(), "Summary preview");
}

#[test]
fn sync_updates_track_folder_progress() {
    let mut status = NativeSyncStatus::default();

    apply_sync_update(&mut status, NativeSyncUpdate::Connected);
    assert!(status.connected);
    assert_eq!(sync_status_title(&status), "Waiting for sync activity");

    apply_sync_update(
        &mut status,
        NativeSyncUpdate::Event(SyncEvent::SyncStart {
            account_id: "account".to_string(),
            folder_count: Some(4),
        }),
    );
    assert!(status.syncing);
    assert!(sync_status_visible(&status));
    assert_eq!(status.total, 4);
    assert_eq!(sync_progress_ratio(&status), 0.0);

    apply_sync_update(
        &mut status,
        NativeSyncUpdate::Event(SyncEvent::SyncFolderDone {
            account_id: "account".to_string(),
            folder: Some("INBOX".to_string()),
            done: Some(2),
            total: Some(4),
        }),
    );
    assert_eq!(status.folder.as_deref(), Some("INBOX"));
    assert_eq!(sync_progress_ratio(&status), 0.5);

    apply_sync_update(
        &mut status,
        NativeSyncUpdate::Event(SyncEvent::SyncComplete {
            account_id: "account".to_string(),
        }),
    );
    assert!(!status.syncing);
    assert_eq!(status.done, 4);
    assert_eq!(sync_status_title(&status), "Sync complete");
    assert!(sync_status_visible(&status));
    assert!(status.completed_at.is_some());
}

#[test]
fn sync_completion_visibility_expires_after_web_delay() {
    let mut status = NativeSyncStatus::default();
    apply_sync_update(
        &mut status,
        NativeSyncUpdate::Event(SyncEvent::SyncStart {
            account_id: "account".to_string(),
            folder_count: Some(1),
        }),
    );
    apply_sync_update(
        &mut status,
        NativeSyncUpdate::Event(SyncEvent::SyncComplete {
            account_id: "account".to_string(),
        }),
    );
    let completed_at = status.completed_at.expect("sync completion timestamp");

    assert!(!expire_sync_completion(
        &mut status,
        completed_at + SYNC_COMPLETE_VISIBLE_FOR - Duration::from_millis(1),
    ));
    assert!(sync_status_visible(&status));
    assert!(expire_sync_completion(
        &mut status,
        completed_at + SYNC_COMPLETE_VISIBLE_FOR,
    ));
    assert!(!sync_status_visible(&status));
    assert_eq!(status.last_message, None);
    assert_eq!(status.done, 0);
    assert_eq!(status.total, 0);
}

#[test]
fn sse_parser_handles_split_sync_events() {
    let mut buffer = String::new();
    let first = drain_sse_sync_events(
        &mut buffer,
        "event: sync\ndata: {\"type\":\"sync_start\",\"accountId\":\"a",
    );
    assert!(first.is_empty());

    let events = drain_sse_sync_events(
        &mut buffer,
        "1\",\"folderCount\":3}\n\nevent: sync\ndata: {\"type\":\"sync_complete\",\"accountId\":\"a1\"}\n\n",
    );

    assert_eq!(
        events,
        vec![
            Ok(SyncEvent::SyncStart {
                account_id: "a1".to_string(),
                folder_count: Some(3),
            }),
            Ok(SyncEvent::SyncComplete {
                account_id: "a1".to_string(),
            }),
        ]
    );
    assert!(buffer.is_empty());
}

#[test]
fn sse_parser_handles_crlf_event_boundaries() {
    let mut buffer = String::new();
    let events = drain_sse_sync_events(
        &mut buffer,
        "event: sync\r\ndata: {\"type\":\"sync_folder_done\",\"accountId\":\"a1\",\"folder\":\"INBOX\",\"done\":2,\"total\":5}\r\n\r\n",
    );

    assert_eq!(
        events,
        vec![Ok(SyncEvent::SyncFolderDone {
            account_id: "a1".to_string(),
            folder: Some("INBOX".to_string()),
            done: Some(2),
            total: Some(5),
        })]
    );
    assert!(buffer.is_empty());
}

#[test]
fn sync_refresh_preserves_selected_message_when_still_present() {
    let mut state = NativeAppState {
        accounts: vec![account("old")],
        accounts_resource: AsyncResource::ready(vec![account("old")]),
        folders_by_account: BTreeMap::from([("account".to_string(), vec![folder("inbox", 1)])]),
        selected_folder: Some("inbox".to_string()),
        selected_message: Some("selected".to_string()),
        selected_full_message: Some(full_message("selected", "inbox", true, false)),
        messages: vec![message("selected", "inbox", true)],
        messages_resource: AsyncResource::ready(vec![message("selected", "inbox", true)]),
        ..NativeAppState::default()
    };

    let request = begin_sync_complete_refresh(&mut state);
    assert!(state.accounts_resource.is_loading());
    assert!(state.messages_resource.is_loading());
    assert_eq!(
        request.source,
        Some(NativeMessageSource::Folder("inbox".to_string()))
    );

    let loaded = LoadedSyncRefresh {
        accounts: vec![account("account")],
        folders_by_account: BTreeMap::from([("account".to_string(), vec![folder("inbox", 2)])]),
        labels: Vec::new(),
        messages: Some(LoadedMessageCollection {
            messages: vec![
                message("newer", "inbox", true),
                message("selected", "inbox", true),
            ],
            calendar_events: Vec::new(),
            selected_message: Some("selected".to_string()),
            selected_full_message: Some(full_message("selected", "inbox", true, false)),
            selected_message_labels: vec![label("important")],
        }),
        move_summary: None,
    };

    assert!(apply_loaded_sync_refresh(&mut state, request, loaded));
    assert_eq!(state.accounts, vec![account("account")]);
    assert_eq!(state.selected_message.as_deref(), Some("selected"));
    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.selected_message_labels, vec![label("important")]);
    assert_eq!(total_unread_count(&state), 2);
    assert!(state.last_error.is_none());
}

#[test]
fn stale_sync_refresh_is_ignored() {
    let mut state = NativeAppState {
        accounts_resource: AsyncResource::ready(vec![account("old")]),
        selected_folder: Some("inbox".to_string()),
        messages_resource: AsyncResource::ready(vec![message("old", "inbox", true)]),
        ..NativeAppState::default()
    };

    let stale = begin_sync_complete_refresh(&mut state);
    let _current = begin_sync_complete_refresh(&mut state);
    let loaded = LoadedSyncRefresh {
        accounts: vec![account("new")],
        folders_by_account: BTreeMap::from([("account".to_string(), vec![folder("inbox", 4)])]),
        labels: Vec::new(),
        messages: Some(LoadedMessageCollection {
            messages: vec![message("new", "inbox", true)],
            calendar_events: Vec::new(),
            selected_message: Some("new".to_string()),
            selected_full_message: Some(full_message("new", "inbox", true, false)),
            selected_message_labels: Vec::new(),
        }),
        move_summary: None,
    };

    assert!(!apply_loaded_sync_refresh(&mut state, stale, loaded));
    assert!(state.accounts.is_empty());
    assert!(state.messages.is_empty());
}

#[test]
fn applying_read_full_message_updates_summary_and_folder_unread() {
    let mut state = NativeAppState::default();
    state
        .folders_by_account
        .insert("account".to_string(), vec![folder("inbox", 1)]);
    state.messages.push(message("message", "inbox", false));

    apply_full_message(&mut state, full_message("message", "inbox", true, true));

    assert!(state.messages[0].is_read);
    assert!(state.messages[0].is_flagged);
    assert_eq!(total_unread_count(&state), 0);
}

#[test]
fn beginning_collection_load_sets_route_and_loading_state() {
    let mut state = NativeAppState::default();
    state.messages.push(message("old", "inbox", false));
    state.selected_message = Some("old".to_string());
    state.selected_full_message = Some(full_message("old", "inbox", false, false));
    state.selected_message_labels = vec![label("old-label")];
    state.selected_message_labels_resource = AsyncResource::ready(vec![label("old-label")]);
    state.search_event_results = vec![calendar_search_result("event", "2026-05-26T14:00:00Z")];

    let request = begin_message_collection_load(
        &mut state,
        &NativeMessageSource::Label("label-a".to_string()),
    );

    assert_eq!(state.selected_view, SelectedView::Mail);
    assert_eq!(state.selected_folder, None);
    assert_eq!(state.selected_label.as_deref(), Some("label-a"));
    assert!(state.messages.is_empty());
    assert_eq!(state.selected_message, None);
    assert!(state.selected_full_message.is_none());
    assert!(state.selected_message_resource.is_idle());
    assert!(state.selected_message_labels.is_empty());
    assert!(state.selected_message_labels_resource.is_idle());
    assert!(state.search_event_results.is_empty());
    assert_eq!(state.messages_resource.current_request(), Some(request));
    assert!(state.messages_resource.is_loading());
}

#[test]
fn stale_collection_load_is_ignored() {
    let mut state = NativeAppState::default();
    let stale = begin_message_collection_load(
        &mut state,
        &NativeMessageSource::Folder("inbox".to_string()),
    );
    let current = begin_message_collection_load(
        &mut state,
        &NativeMessageSource::Folder("archive".to_string()),
    );

    let loaded = LoadedMessageCollection {
        messages: vec![message("stale", "inbox", false)],
        calendar_events: vec![calendar_search_result(
            "stale-event",
            "2026-05-26T14:00:00Z",
        )],
        selected_message: Some("stale".to_string()),
        selected_full_message: None,
        selected_message_labels: Vec::new(),
    };

    assert!(!apply_loaded_message_collection(&mut state, stale, loaded));
    assert_eq!(state.selected_folder.as_deref(), Some("archive"));
    assert!(state.messages.is_empty());
    assert_eq!(state.messages_resource.current_request(), Some(current));
    assert!(state.messages_resource.is_loading());
}

#[test]
fn current_collection_load_applies_first_full_message() {
    let mut state = NativeAppState::default();
    state
        .folders_by_account
        .insert("account".to_string(), vec![folder("inbox", 1)]);
    let request = begin_message_collection_load(
        &mut state,
        &NativeMessageSource::Folder("inbox".to_string()),
    );
    let loaded = LoadedMessageCollection {
        messages: vec![message("message", "inbox", false)],
        calendar_events: vec![calendar_search_result("event", "2026-05-26T14:00:00Z")],
        selected_message: Some("message".to_string()),
        selected_full_message: Some(full_message("message", "inbox", true, true)),
        selected_message_labels: vec![label("work")],
    };

    assert!(apply_loaded_message_collection(&mut state, request, loaded));
    assert_eq!(state.selected_message.as_deref(), Some("message"));
    assert!(state.selected_full_message.is_some());
    assert!(state.selected_message_resource.is_ready());
    assert!(state.messages[0].is_read);
    assert!(state.messages[0].is_flagged);
    assert_eq!(state.selected_message_labels, vec![label("work")]);
    assert!(state.selected_message_labels_resource.is_ready());
    assert_eq!(state.search_event_results.len(), 1);
    assert_eq!(total_unread_count(&state), 0);
}

#[test]
fn stale_full_message_load_is_ignored() {
    let mut state = NativeAppState::default();
    let stale = begin_full_message_load(&mut state, "first").expect("first request");
    let current = begin_full_message_load(&mut state, "second").expect("second request");

    assert!(!apply_loaded_full_message(
        &mut state,
        stale,
        "first",
        LoadedFullMessage {
            message: full_message("first", "inbox", true, false),
            labels: Vec::new(),
        },
    ));
    assert_eq!(state.selected_message.as_deref(), Some("second"));
    assert_eq!(
        state.selected_message_resource.current_request(),
        Some(current)
    );
    assert!(state.selected_message_resource.is_loading());
    assert!(state.selected_full_message.is_none());
}

#[test]
fn quick_reply_request_defaults_to_selected_sender() {
    let mut state = NativeAppState::default();
    let mut summary = message("message", "inbox", true);
    summary.subject = Some("Status".to_string());
    summary.from_name = Some("Ada Lovelace".to_string());
    summary.from_email = Some("ada@example.test".to_string());
    summary.message_id = Some("<message@example.test>".to_string());
    state.selected_message = Some("message".to_string());
    state.messages = vec![summary];

    let request = build_quick_reply_request(&state, Vec::new(), "  Thanks  ".to_string()).unwrap();

    assert_eq!(request.account_id, "account");
    assert_eq!(request.to, vec!["ada@example.test"]);
    assert_eq!(request.subject, "Re: Status");
    assert_eq!(request.text_body.as_deref(), Some("Thanks"));
    assert_eq!(
        request.in_reply_to.as_deref(),
        Some("<message@example.test>")
    );
}

#[test]
fn inline_reply_quotes_the_original_message() {
    let mut state = NativeAppState::default();
    let mut full = full_message("message", "inbox", true, false);
    full.subject = Some("Status".to_string());
    full.from_name = Some("Ada Lovelace".to_string());
    full.from_email = Some("ada@example.test".to_string());
    full.body = Some(MessageBody {
        html_body: None,
        text_body: Some("Line one\nLine two".to_string()),
        ..Default::default()
    });
    state.selected_message = Some("message".to_string());
    state.selected_full_message = Some(full);

    let request = build_quick_reply_request(&state, Vec::new(), "Thanks".to_string()).unwrap();

    assert_eq!(
        request.text_body.as_deref(),
        Some("Thanks\n\nAda Lovelace wrote:\n> Line one\n> Line two")
    );
}

#[test]
fn inline_reply_carries_text_and_recipients_into_the_composer() {
    let mut compose = ComposeState {
        open: true,
        mode: Some("reply".to_string()),
        to: vec!["ada@example.test".to_string()],
        ..ComposeState::default()
    };
    assert!(carry_inline_reply(
        &mut compose,
        vec![
            " bob@example.test ".to_string(),
            "BOB@example.test".to_string()
        ],
        "Draft so far".to_string(),
    ));
    assert_eq!(compose.to, vec!["bob@example.test"]);
    assert_eq!(compose.body, "Draft so far");

    // No typed recipients: keep the reply seed's recipient.
    let mut compose = ComposeState {
        open: true,
        to: vec!["ada@example.test".to_string()],
        ..ComposeState::default()
    };
    carry_inline_reply(&mut compose, Vec::new(), "x".to_string());
    assert_eq!(compose.to, vec!["ada@example.test"]);
}

#[test]
fn inline_reply_is_not_carried_when_no_composer_opened() {
    let mut compose = ComposeState::default();
    assert!(!carry_inline_reply(
        &mut compose,
        vec!["bob@example.test".to_string()],
        "text".to_string()
    ));
    assert!(compose.body.is_empty());
    assert!(compose.to.is_empty());
}

#[test]
fn forward_request_uses_explicit_recipient_and_omits_in_reply_to() {
    let mut state = NativeAppState::default();
    let mut full = full_message("message", "inbox", true, false);
    full.subject = Some("Plans".to_string());
    full.from_name = Some("Ada Lovelace".to_string());
    full.from_email = Some("ada@example.test".to_string());
    full.message_id = Some("<message@example.test>".to_string());
    state.selected_message = Some("message".to_string());
    state.selected_full_message = Some(full);
    state.compose.open = true;
    state.compose.mode = Some("forward".to_string());
    state.compose.account_id = Some("account".to_string());
    state.compose.subject = Some("Fwd: Plans".to_string());

    let request = build_quick_reply_request(
        &state,
        vec![
            " bob@example.test ".to_string(),
            "BOB@example.test".to_string(),
        ],
        "See below".to_string(),
    )
    .unwrap();

    assert_eq!(request.to, vec!["bob@example.test"]);
    assert_eq!(request.subject, "Fwd: Plans");
    assert_eq!(request.in_reply_to, None);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn forward_request_requires_a_recipient() {
    let mut state = NativeAppState::default();
    state.selected_message = Some("message".to_string());
    state.selected_full_message = Some(full_message("message", "inbox", true, false));
    state.compose.mode = Some("forward".to_string());

    let err = build_quick_reply_request(&state, Vec::new(), "Body".to_string()).unwrap_err();

    assert_eq!(err, "Add at least one recipient.");
}

#[test]
fn compose_reply_request_appends_quote_and_signature() {
    let mut state = NativeAppState::default();
    let mut full = full_message("message", "inbox", true, false);
    full.subject = Some("Status".to_string());
    full.from_name = Some("Ada Lovelace".to_string());
    full.from_email = Some("ada@example.test".to_string());
    full.message_id = Some("<message@example.test>".to_string());
    full.body = Some(MessageBody {
        html_body: None,
        text_body: Some("Original line\nSecond line".to_string()),
        ..Default::default()
    });
    state.selected_message = Some("message".to_string());
    state.selected_full_message = Some(full);
    state.compose.open = true;
    state.compose.mode = Some("reply".to_string());
    state.compose.account_id = Some("account".to_string());
    state.compose.subject = Some("Re: Status".to_string());
    state.compose.in_reply_to = Some("<message@example.test>".to_string());
    state.compose.quoted_text = Some("Original line\nSecond line".to_string());
    state.compose.quoted_from = Some("Ada Lovelace".to_string());
    state.compose.signature = Some("Regards,\nAda".to_string());

    let request = build_compose_request(
        &state,
        vec![" ada@example.test ".to_string()],
        vec![" team@example.test ".to_string()],
        Vec::new(),
        " Re: Status ".to_string(),
        "  Thanks  ".to_string(),
    )
    .unwrap();

    assert_eq!(request.to, vec!["ada@example.test"]);
    assert_eq!(request.cc, vec!["team@example.test"]);
    assert_eq!(request.subject, "Re: Status");
    assert_eq!(
        request.text_body.as_deref(),
        Some("Thanks\n\nAda Lovelace wrote:\n> Original line\n> Second line\n\n-- \nRegards,\nAda")
    );
    assert_eq!(
        request.in_reply_to.as_deref(),
        Some("<message@example.test>")
    );
}

#[test]
fn compose_forward_request_omits_in_reply_to_and_normalizes_recipients() {
    let mut state = NativeAppState::default();
    state.compose.open = true;
    state.compose.mode = Some("forward".to_string());
    state.compose.account_id = Some("account".to_string());
    state.compose.subject = Some("Fwd: Plans".to_string());
    state.compose.in_reply_to = Some("<ignored@example.test>".to_string());

    let request = build_compose_request(
        &state,
        vec![
            " bob@example.test ".to_string(),
            "BOB@example.test".to_string(),
        ],
        vec![" cc@example.test ".to_string()],
        vec![" audit@example.test ".to_string()],
        "Fwd: Plans".to_string(),
        "See below".to_string(),
    )
    .unwrap();

    assert_eq!(request.to, vec!["bob@example.test"]);
    assert_eq!(request.cc, vec!["cc@example.test"]);
    assert_eq!(request.bcc, vec!["audit@example.test"]);
    assert_eq!(request.in_reply_to, None);
    assert_eq!(request.text_body.as_deref(), Some("See below"));
}

#[test]
fn compose_request_waits_for_signature_load() {
    let mut state = NativeAppState::default();
    state.compose.open = true;
    state.compose.mode = Some("compose".to_string());
    state.compose.account_id = Some("account".to_string());
    state.compose.signature_loading = true;

    let err = build_compose_request(
        &state,
        vec!["bob@example.test".to_string()],
        Vec::new(),
        Vec::new(),
        "Subject".to_string(),
        "Body".to_string(),
    )
    .unwrap_err();

    assert_eq!(err, "Wait for the account signature to load.");
}

#[test]
fn settings_update_request_trims_name_and_sets_optional_values() {
    let request = build_account_settings_update_request(
        "  Ada  ".to_string(),
        "30".to_string(),
        "Regards,\nAda".to_string(),
    )
    .unwrap();

    assert_eq!(request.name.as_deref(), Some("Ada"));
    assert_eq!(request.sync_days_limit, Some(Some(30)));
    assert_eq!(request.signature, Some(Some("Regards,\nAda".to_string())));
}

#[test]
fn settings_update_request_clears_blank_optional_values() {
    let request = build_account_settings_update_request(
        "Ada".to_string(),
        " ".to_string(),
        "\n  ".to_string(),
    )
    .unwrap();

    assert_eq!(request.sync_days_limit, Some(None));
    assert_eq!(request.signature, Some(None));
}

#[test]
fn settings_update_request_rejects_invalid_values() {
    let err =
        build_account_settings_update_request(" ".to_string(), "30".to_string(), "Sig".to_string())
            .unwrap_err();
    assert_eq!(err, "Enter a display name.");

    let err = build_account_settings_update_request(
        "Ada".to_string(),
        "0".to_string(),
        "Sig".to_string(),
    )
    .unwrap_err();
    assert_eq!(err, "Download days must be a positive whole number.");
}

fn account_setup_draft() -> AccountSetupFormDraft {
    AccountSetupFormDraft {
        name: "  Ada  ".to_string(),
        email: "  ada@example.test  ".to_string(),
        imap_host: "  imap.example.test  ".to_string(),
        imap_port: "993".to_string(),
        imap_password: "imap-secret".to_string(),
        smtp_host: "  smtp.example.test  ".to_string(),
        smtp_port: "587".to_string(),
        smtp_password: "smtp-secret".to_string(),
        same_password: true,
    }
}

#[test]
fn manual_account_request_trims_fields_and_reuses_imap_password() {
    let request = build_manual_account_request(account_setup_draft()).unwrap();

    assert_eq!(request.name, "Ada");
    assert_eq!(request.email, "ada@example.test");
    assert_eq!(request.provider_type, "imap");
    assert_eq!(request.auth_type, "basic");
    assert_eq!(request.host.as_deref(), Some("imap.example.test"));
    assert_eq!(request.port, Some(993));
    assert_eq!(request.password.as_deref(), Some("imap-secret"));
    assert_eq!(request.smtp_host.as_deref(), Some("smtp.example.test"));
    assert_eq!(request.smtp_port, Some(587));
    assert_eq!(request.smtp_password.as_deref(), Some("imap-secret"));
    assert_eq!(request.use_ssl, None);
}

#[test]
fn manual_account_request_uses_distinct_smtp_password() {
    let mut draft = account_setup_draft();
    draft.same_password = false;

    let request = build_manual_account_request(draft).unwrap();

    assert_eq!(request.password.as_deref(), Some("imap-secret"));
    assert_eq!(request.smtp_password.as_deref(), Some("smtp-secret"));
}

#[test]
fn microsoft_app_password_request_maps_defaults() {
    let mut draft = account_setup_draft();
    draft.imap_host = "outlook.office365.com".to_string();
    draft.smtp_host = "smtp.office365.com".to_string();
    draft.imap_password = "app-secret".to_string();

    let request = build_microsoft_app_password_request(draft).unwrap();

    assert_eq!(request.provider_type, "microsoft365");
    assert_eq!(request.auth_type, "app_password");
    assert_eq!(request.host.as_deref(), Some("outlook.office365.com"));
    assert_eq!(request.port, Some(993));
    assert_eq!(request.password.as_deref(), Some("app-secret"));
    assert_eq!(request.smtp_host.as_deref(), Some("smtp.office365.com"));
    assert_eq!(request.smtp_port, Some(587));
    assert_eq!(request.smtp_password.as_deref(), Some("app-secret"));
}

#[test]
fn account_setup_request_rejects_invalid_values() {
    let mut draft = account_setup_draft();
    draft.email = "ada".to_string();
    let err = build_manual_account_request(draft).unwrap_err();
    assert_eq!(err, "Enter a valid email address.");

    let mut draft = account_setup_draft();
    draft.imap_port = "0".to_string();
    let err = build_manual_account_request(draft).unwrap_err();
    assert_eq!(err, "IMAP port must be between 1 and 65535.");

    let mut draft = account_setup_draft();
    draft.same_password = false;
    draft.smtp_password = " ".to_string();
    let err = build_manual_account_request(draft).unwrap_err();
    assert_eq!(err, "Enter a SMTP password.");
}

#[test]
fn account_create_reload_closes_setup_on_first_run() {
    let mut state = NativeAppState {
        theme: ThemeMode::Light,
        density: DensityTokens::new(6),
        modals: ModalStates {
            account_setup_open: true,
            settings_open: true,
            advanced_search_open: true,
        },
        palette: PaletteState {
            open: true,
            query: "account".to_string(),
        },
        ..NativeAppState::default()
    };
    let loaded = NativeAppState {
        api_base_url: "http://localhost:3000".to_string(),
        accounts: vec![account("new")],
        accounts_resource: AsyncResource::ready(vec![account("new")]),
        selected_folder: Some("inbox".to_string()),
        selected_message: Some("message".to_string()),
        messages: vec![message("message", "inbox", true)],
        messages_resource: AsyncResource::ready(vec![message("message", "inbox", true)]),
        theme: ThemeMode::Dark,
        density: DensityTokens::new(1),
        ..NativeAppState::default()
    };

    apply_account_create_reload(&mut state, loaded);

    assert_eq!(state.accounts, vec![account("new")]);
    assert_eq!(state.theme, ThemeMode::Light);
    assert_eq!(state.density.level, 6);
    assert_eq!(state.selected_view, SelectedView::Mail);
    assert_eq!(state.selected_folder.as_deref(), Some("inbox"));
    assert_eq!(state.selected_message.as_deref(), Some("message"));
    assert!(!state.modals.account_setup_open);
    assert!(!state.modals.settings_open);
    assert!(!state.modals.advanced_search_open);
    assert!(state.palette.open);
    assert!(state.last_error.is_none());
}

#[test]
fn account_create_reload_preserves_existing_workspace_selection() {
    let mut state = NativeAppState {
        accounts: vec![account("old")],
        accounts_resource: AsyncResource::ready(vec![account("old")]),
        selected_view: SelectedView::Calendar,
        selected_folder: Some("old-inbox".to_string()),
        selected_label: Some("label-a".to_string()),
        selected_message: Some("old-message".to_string()),
        selected_full_message: Some(full_message("old-message", "old-inbox", true, false)),
        selected_event: Some("event-1".to_string()),
        search: SearchState {
            query: "status".to_string(),
            submitted_query: Some("status".to_string()),
            ..SearchState::default()
        },
        messages: vec![message("old-message", "old-inbox", true)],
        messages_resource: AsyncResource::ready(vec![message("old-message", "old-inbox", true)]),
        compose: ComposeState {
            open: true,
            account_id: Some("old".to_string()),
            ..ComposeState::default()
        },
        modals: ModalStates {
            account_setup_open: true,
            settings_open: false,
            advanced_search_open: true,
        },
        last_error: Some("old".to_string()),
        ..NativeAppState::default()
    };
    let loaded = NativeAppState {
        api_base_url: "http://localhost:3000".to_string(),
        accounts: vec![account("old"), account("new")],
        accounts_resource: AsyncResource::ready(vec![account("old"), account("new")]),
        selected_view: SelectedView::Mail,
        selected_folder: Some("new-inbox".to_string()),
        selected_message: Some("new-message".to_string()),
        messages: vec![message("new-message", "new-inbox", true)],
        messages_resource: AsyncResource::ready(vec![message("new-message", "new-inbox", true)]),
        ..NativeAppState::default()
    };

    apply_account_create_reload(&mut state, loaded);

    assert_eq!(state.accounts, vec![account("old"), account("new")]);
    assert_eq!(state.selected_view, SelectedView::Calendar);
    assert_eq!(state.selected_folder.as_deref(), Some("old-inbox"));
    assert_eq!(state.selected_label.as_deref(), Some("label-a"));
    assert_eq!(state.selected_message.as_deref(), Some("old-message"));
    assert_eq!(state.selected_event.as_deref(), Some("event-1"));
    assert_eq!(state.search.submitted_query.as_deref(), Some("status"));
    assert_eq!(
        state.messages,
        vec![message("old-message", "old-inbox", true)]
    );
    assert!(state.compose.open);
    assert!(!state.modals.account_setup_open);
    assert!(!state.modals.advanced_search_open);
    assert!(state.last_error.is_none());
}

#[test]
fn account_delete_reload_preserves_native_state_and_resets_deleted_compose() {
    let mut state = NativeAppState {
        theme: ThemeMode::Light,
        density: DensityTokens::new(7),
        selected_view: SelectedView::Calendar,
        selected_event: Some("event-1".to_string()),
        compose: ComposeState {
            open: true,
            account_id: Some("deleted".to_string()),
            revision: 4,
            ..ComposeState::default()
        },
        modals: ModalStates {
            account_setup_open: true,
            settings_open: true,
            advanced_search_open: true,
        },
        palette: PaletteState {
            open: true,
            query: "settings".to_string(),
        },
        sync: NativeSyncStatus {
            connected: true,
            ..NativeSyncStatus::default()
        },
        last_error: Some("old".to_string()),
        ..NativeAppState::default()
    };
    let loaded = NativeAppState {
        api_base_url: "http://localhost:3000".to_string(),
        accounts: vec![account("remaining")],
        accounts_resource: AsyncResource::ready(vec![account("remaining")]),
        theme: ThemeMode::Dark,
        density: DensityTokens::new(1),
        selected_view: SelectedView::Mail,
        ..NativeAppState::default()
    };

    apply_account_delete_reload(&mut state, "deleted", loaded);

    assert_eq!(state.accounts, vec![account("remaining")]);
    assert_eq!(state.theme, ThemeMode::Light);
    assert_eq!(state.density.level, 7);
    assert_eq!(state.selected_view, SelectedView::Calendar);
    assert_eq!(state.selected_event.as_deref(), Some("event-1"));
    assert!(state.modals.account_setup_open);
    assert!(!state.modals.settings_open);
    assert!(!state.modals.advanced_search_open);
    assert!(state.palette.open);
    assert!(state.sync.connected);
    assert!(!state.compose.open);
    assert_eq!(state.compose.revision, 5);
    assert!(state.last_error.is_none());
}

#[test]
fn webhook_create_request_trims_fields_and_defaults_events() {
    let request = build_create_webhook_request(
        "  https://example.test/hook  ".to_string(),
        "  ".to_string(),
        "".to_string(),
    )
    .unwrap();

    assert_eq!(request.url, "https://example.test/hook");
    assert_eq!(request.secret, None);
    assert_eq!(request.events.as_deref(), Some("new_message"));
    assert_eq!(request.account_id, None);
}

#[test]
fn webhook_create_request_accepts_secret_and_all_events() {
    let request = build_create_webhook_request(
        "https://example.test/hook".to_string(),
        "  secret  ".to_string(),
        "all".to_string(),
    )
    .unwrap();

    assert_eq!(request.secret.as_deref(), Some("secret"));
    assert_eq!(request.events.as_deref(), Some("all"));
}

#[test]
fn webhook_create_request_rejects_invalid_values() {
    let err =
        build_create_webhook_request(" ".to_string(), "".to_string(), "new_message".to_string())
            .unwrap_err();
    assert_eq!(err, "Enter a webhook URL.");

    let err = build_create_webhook_request(
        "https://example.test/hook".to_string(),
        "".to_string(),
        "message_deleted".to_string(),
    )
    .unwrap_err();
    assert_eq!(err, "Choose a supported webhook event set.");
}

#[test]
fn rule_save_request_builds_new_rule_payload() {
    let request = build_settings_rule_save_request(
        Some(SettingsRuleEditorMode::New),
        Some(" account ".to_string()),
        "  Move Ada reports  ".to_string(),
        MatchMode::Any,
        vec![
            Condition {
                id: "condition-1".to_string(),
                field: ConditionField::Subject,
                operator: ConditionOperator::StartsWith,
                value: " Report ".to_string(),
            },
            Condition {
                id: "condition-2".to_string(),
                field: ConditionField::IsFlagged,
                operator: ConditionOperator::Contains,
                value: "".to_string(),
            },
        ],
        vec![SettingsRuleActionDraft {
            action_type: "move_to_folder".to_string(),
            action_value: Some(" archive ".to_string()),
        }],
    )
    .unwrap();

    assert_eq!(request.mode, SettingsRuleEditorMode::New);
    assert_eq!(request.account_id, "account");
    assert_eq!(request.name, "Move Ada reports");
    assert_eq!(request.match_mode, "any");
    assert_eq!(
        request.conditions,
        vec![
            ConditionInput {
                field: "subject".to_string(),
                operator: "starts_with".to_string(),
                value: "Report".to_string(),
            },
            ConditionInput {
                field: "is_flagged".to_string(),
                operator: "contains".to_string(),
                value: "true".to_string(),
            },
        ]
    );
    assert_eq!(
        request.actions,
        vec![ActionInput {
            action_type: "move_to_folder".to_string(),
            action_value: Some("archive".to_string()),
        }]
    );
}

#[test]
fn rule_save_request_builds_edit_payload_and_clears_non_move_values() {
    let request = build_settings_rule_save_request(
        Some(SettingsRuleEditorMode::Edit("rule-1".to_string())),
        Some("account".to_string()),
        "Mark read".to_string(),
        MatchMode::All,
        vec![Condition {
            id: "condition-1".to_string(),
            field: ConditionField::From,
            operator: ConditionOperator::Contains,
            value: "ada@example.test".to_string(),
        }],
        vec![SettingsRuleActionDraft {
            action_type: "mark_read".to_string(),
            action_value: Some("ignored-folder".to_string()),
        }],
    )
    .unwrap();

    assert_eq!(
        request.mode,
        SettingsRuleEditorMode::Edit("rule-1".to_string())
    );
    assert_eq!(request.match_mode, "all");
    assert_eq!(
        request.actions,
        vec![ActionInput {
            action_type: "mark_read".to_string(),
            action_value: None,
        }]
    );
}

#[test]
fn rule_save_request_rejects_invalid_values() {
    let err = build_settings_rule_save_request(
        Some(SettingsRuleEditorMode::New),
        Some("account".to_string()),
        " ".to_string(),
        MatchMode::All,
        vec![new_advanced_condition("condition-1".to_string())],
        vec![SettingsRuleActionDraft {
            action_type: "mark_read".to_string(),
            action_value: None,
        }],
    )
    .unwrap_err();
    assert_eq!(err, "Enter a rule name.");

    let err = build_settings_rule_save_request(
        Some(SettingsRuleEditorMode::New),
        Some("account".to_string()),
        "Move".to_string(),
        MatchMode::All,
        vec![new_advanced_condition("condition-1".to_string())],
        vec![SettingsRuleActionDraft {
            action_type: "move_to_folder".to_string(),
            action_value: None,
        }],
    )
    .unwrap_err();
    assert_eq!(err, "Choose a folder for move actions.");

    let err = build_settings_rule_save_request(
        Some(SettingsRuleEditorMode::New),
        Some("account".to_string()),
        "Unsupported".to_string(),
        MatchMode::All,
        vec![new_advanced_condition("condition-1".to_string())],
        vec![SettingsRuleActionDraft {
            action_type: "bounce".to_string(),
            action_value: None,
        }],
    )
    .unwrap_err();
    assert_eq!(err, "Choose a supported rule action.");
}

#[test]
fn rule_counts_text_pluralizes_counts() {
    let mut rule = RuleDto {
        id: "rule".to_string(),
        account_id: "account".to_string(),
        name: "Rule".to_string(),
        is_active: true,
        match_mode: "all".to_string(),
        priority: 0,
        conditions: vec![RuleConditionDto {
            id: "condition".to_string(),
            field: "from".to_string(),
            operator: "contains".to_string(),
            value: "ada".to_string(),
        }],
        actions: vec![RuleActionDto {
            id: "action".to_string(),
            action_type: "mark_read".to_string(),
            action_value: None,
        }],
    };
    assert_eq!(rule_counts_text(&rule), "1 condition, 1 action");

    rule.conditions.push(RuleConditionDto {
        id: "condition-2".to_string(),
        field: "subject".to_string(),
        operator: "contains".to_string(),
        value: "status".to_string(),
    });
    rule.actions.clear();
    assert_eq!(rule_counts_text(&rule), "2 conditions, 0 actions");
}

/// A one-thread HTTP stub for `POST /api/v1/messages/{id}/move`. Each request
/// is answered by `respond(path, body)`; `(status, json)` is sent back.
/// Returns the base URL and the `(path, body)` of every request, in order.
fn move_stub(
    requests: usize,
    respond: impl Fn(&str, &str) -> (u16, String) + Send + 'static,
) -> (String, std::thread::JoinHandle<Vec<(String, String)>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            let body = loop {
                let n = stream.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    if body.contains('}') || n == 0 {
                        let _ = head;
                        break body.to_string();
                    }
                }
                if n == 0 {
                    break String::new();
                }
            };
            let text = String::from_utf8_lossy(&buf).to_string();
            let path = text
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            let (status, json) = respond(&path, &body);
            let reason = if status == 200 { "OK" } else { "Error" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                json.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            seen.push((path, body));
        }
        seen
    });
    (base, handle)
}

fn run_async<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn multi_move_reports_each_message_and_keeps_going_after_a_failure() {
    let (base, server) = move_stub(3, |path, _| match path {
        "/api/v1/messages/a/move" => (200, r#"{"status":"moved","messageId":"a2"}"#.into()),
        "/api/v1/messages/b/move" => (502, r#"{"error":"provider said no"}"#.into()),
        _ => (200, r#"{"status":"moved","messageId":"c2"}"#.into()),
    });
    let client = EmailClient::new(&base).unwrap();
    let items = ["a", "b", "c"]
        .map(|id| (id.to_string(), "projects".to_string(), "inbox".to_string()))
        .to_vec();

    let summary = run_async(move_each(&client, items, false));

    // The failure on `b` did not stop `c`.
    let seen = server.join().unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(
        summary.moved,
        vec![
            MovedRef {
                message_id: "a2".to_string(),
                original_folder_id: "inbox".to_string()
            },
            MovedRef {
                message_id: "c2".to_string(),
                original_folder_id: "inbox".to_string()
            },
        ]
    );
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].0, "b");
    assert!(summary.can_undo());
    let text = summary.text();
    assert!(
        text.starts_with("Moved 2 of 3 messages; 1 failed"),
        "{text}"
    );
}

#[test]
fn undo_moves_each_message_back_by_its_new_id() {
    let (base, server) = move_stub(2, |_, _| {
        (200, r#"{"status":"moved","messageId":"back"}"#.into())
    });
    let client = EmailClient::new(&base).unwrap();
    let summary = MoveSummary {
        moved: vec![
            MovedRef {
                message_id: "a2".to_string(),
                original_folder_id: "inbox".to_string(),
            },
            MovedRef {
                message_id: "c2".to_string(),
                original_folder_id: "archive".to_string(),
            },
        ],
        ..MoveSummary::default()
    };
    let items = summary
        .moved
        .iter()
        .map(|m| {
            (
                m.message_id.clone(),
                m.original_folder_id.clone(),
                String::new(),
            )
        })
        .collect();

    let undone = run_async(move_each(&client, items, true));

    let seen = server.join().unwrap();
    assert_eq!(seen[0].0, "/api/v1/messages/a2/move");
    assert!(seen[0].1.contains(r#""folderId":"inbox""#), "{:?}", seen[0]);
    assert_eq!(seen[1].0, "/api/v1/messages/c2/move");
    assert!(
        seen[1].1.contains(r#""folderId":"archive""#),
        "{:?}",
        seen[1]
    );
    assert_eq!(undone.moved_count(), 2);
    assert!(undone.failed.is_empty());
    assert!(!undone.can_undo(), "an undo is not itself undoable");
}

#[test]
fn move_summary_text_covers_all_outcomes() {
    let moved = |n: usize| {
        (0..n)
            .map(|i| MovedRef {
                message_id: format!("m{i}"),
                original_folder_id: "inbox".to_string(),
            })
            .collect::<Vec<_>>()
    };
    let all = MoveSummary {
        moved: moved(1),
        ..MoveSummary::default()
    };
    assert_eq!(all.text(), "Moved 1 message.");
    let none = MoveSummary {
        failed: vec![("a".into(), "boom".into()), ("b".into(), "bang".into())],
        ..MoveSummary::default()
    };
    assert_eq!(none.text(), "Could not move 2 messages: boom (and 1 more)");
    assert!(!none.can_undo());
    let untracked = MoveSummary {
        moved_untracked: 2,
        ..MoveSummary::default()
    };
    assert!(untracked.text().contains("Undo is unavailable for 2"));
    assert!(!untracked.can_undo());
}

#[test]
fn move_outcome_is_kept_even_when_the_refresh_is_stale() {
    let mut state = NativeAppState::default();
    let stale = begin_sync_complete_refresh(&mut state);
    begin_sync_complete_refresh(&mut state);
    let loaded = LoadedSyncRefresh {
        accounts: Vec::new(),
        folders_by_account: BTreeMap::new(),
        labels: Vec::new(),
        messages: None,
        move_summary: Some(MoveSummary {
            moved: vec![MovedRef {
                message_id: "a2".to_string(),
                original_folder_id: "inbox".to_string(),
            }],
            ..MoveSummary::default()
        }),
    };
    assert!(!apply_loaded_sync_refresh(&mut state, stale, loaded));
    assert!(state.move_notice.as_ref().unwrap().summary.can_undo());

    let mut notice = state.move_notice.clone();
    assert!(!expire_move_notice(&mut notice, Instant::now()));
    assert!(expire_move_notice(
        &mut notice,
        Instant::now() + MOVE_NOTICE_LIFETIME
    ));
    assert!(notice.is_none());
}

#[test]
fn undo_window_preference_is_clamped_on_parse_and_save() {
    assert_eq!(
        parse_native_preferences("undo_window_secs=0\n").undo_window_secs,
        MIN_UNDO_WINDOW_SECS
    );
    assert_eq!(
        parse_native_preferences("undo_window_secs=200\n").undo_window_secs,
        MAX_UNDO_WINDOW_SECS
    );
    assert_eq!(
        parse_native_preferences("undo_window_secs=abc\n").undo_window_secs,
        DEFAULT_UNDO_WINDOW_SECS
    );
    let out_of_range = NativePreferences {
        undo_window_secs: 250,
        ..NativePreferences::default()
    };
    assert!(serialize_native_preferences(&out_of_range).contains("undo_window_secs=30\n"));
}

#[test]
fn send_request_carries_the_configured_undo_window() {
    let mut state = NativeAppState {
        undo_window_secs: 17,
        accounts: vec![account("acct")],
        ..NativeAppState::default()
    };
    state.compose.account_id = Some("acct".to_string());
    let request = build_compose_request(
        &state,
        vec!["bob@example.test".to_string()],
        Vec::new(),
        Vec::new(),
        "Hi".to_string(),
        "Body".to_string(),
    )
    .expect("request");
    assert_eq!(request.send_delay_seconds, Some(17));
}

#[test]
fn needs_reauth_event_shows_reconnect_error_until_reauthed() {
    let mut status = NativeSyncStatus::default();
    let needs = NativeSyncUpdate::Event(SyncEvent::AccountNeedsReauth {
        account_id: "account".to_string(),
        reason: Some("invalid_grant".to_string()),
    });
    assert!(sync_update_requests_refresh(&needs));
    apply_sync_update(&mut status, needs);
    assert!(!status.syncing);
    assert!(status
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("Reconnect")));
    assert!(sync_status_visible(&status));

    let reauthed = NativeSyncUpdate::Event(SyncEvent::AccountReauthed {
        account_id: "account".to_string(),
    });
    assert!(sync_update_requests_refresh(&reauthed));
    apply_sync_update(&mut status, reauthed);
    assert!(status.last_error.is_none());
}
