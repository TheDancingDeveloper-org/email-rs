use crate::prelude::*;
use crate::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn compose_placeholder(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    to_input: Entity<TokenInput>,
    cc_input: Entity<TokenInput>,
    bcc_input: Entity<TokenInput>,
    subject_input: Entity<TextInput>,
    body_input: Entity<Textarea>,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let compose = &state.compose;
    let mode = compose.mode.as_deref().unwrap_or("compose");
    let mode_label = compose_mode_label(mode);
    let header_subject = compose
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|subject| !subject.is_empty())
        .unwrap_or("No subject")
        .to_string();
    let show_cc = compose.show_cc || !cc_input.read(cx).values().is_empty();
    let show_bcc = compose.show_bcc || !bcc_input.read(cx).values().is_empty();
    let has_to = token_input_has_value(&to_input, cx);
    let has_body = !body_input.read(cx).text().trim().is_empty();
    let can_send = !compose.sending && !compose.signature_loading && has_to && has_body;
    let compose_status = compose
        .error
        .clone()
        .map(|error| (error, true))
        .or_else(|| {
            compose
                .signature_loading
                .then(|| ("Loading signature...".to_string(), false))
        })
        .or_else(|| compose.notice.clone().map(|notice| (notice, false)));
    let signature = compose
        .signature
        .clone()
        .filter(|signature| !signature.trim().is_empty());
    let quoted_text = (mode == "reply")
        .then(|| compose.quoted_text.clone())
        .flatten()
        .filter(|quote| !quote.trim().is_empty());
    let quoted_from = compose.quoted_from.clone();
    let mut body_section = div()
        .id("compose-body-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(20.0))
        .py(px(16.0))
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(body_input.clone());
    if let Some(signature) = signature.as_deref() {
        body_section = body_section.child(compose_signature_block(signature, cx));
    }
    if let Some(quote) = quoted_text.as_deref() {
        body_section = body_section.child(compose_quote_block(quote, quoted_from.as_deref(), cx));
    }

    div()
        .size_full()
        .flex()
        .flex_col()
        .when(compose.discard_pending, |view| {
            view.child(
                div()
                    .p(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child("Discard this message? Your unsent changes will be lost.")
                    .child(
                        Button::new("compose-keep-editing")
                            .label("Keep editing")
                            .on_click({
                                let state_entity = state_entity.clone();
                                move |_, _, app_cx| {
                                    resolve_compose_discard(&state_entity, false, app_cx);
                                }
                            }),
                    )
                    .child(
                        Button::new("compose-confirm-discard")
                            .label("Discard message")
                            .disabled(compose.sending)
                            .on_click({
                                let state_entity = state_entity.clone();
                                move |_, _, app_cx| {
                                    resolve_compose_discard(&state_entity, true, app_cx);
                                }
                            }),
                    ),
            )
        })
        .border_l_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.surface)
        .child(
            div()
                .px(px(20.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(colors.stroke_neutral_subtle)
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .h(px(24.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .bg(colors.neutral)
                        .text_size(px(typography.caption.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(mode_label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(typography.subtitle.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(header_subject),
                )
                .child(
                    Button::new("compose-header-discard")
                        .label("Discard")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(compose.sending)
                        .on_click({
                            let state_entity = state_entity.clone();
                            move |_, _, app_cx| {
                                discard_compose(&state_entity, app_cx);
                            }
                        }),
                ),
        )
        .child(
            div()
                .px(px(20.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(colors.stroke_neutral_subtle)
                .flex()
                .flex_col()
                .gap(px(8.0))
                .when(compose.identities.len() > 1, |fields| {
                    fields.child(compose_identity_row(state_entity, compose, cx))
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(compose_field_label("To", cx))
                        .child(div().flex_1().min_w_0().child(to_input.clone()))
                        .when(!show_cc, |row| {
                            row.child(
                                Button::new("compose-show-cc")
                                    .label("Cc")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = state_entity.clone();
                                        move |_, _, app_cx| {
                                            set_compose_cc_visible(&state_entity, true, app_cx);
                                        }
                                    }),
                            )
                        })
                        .when(!show_bcc, |row| {
                            row.child(
                                Button::new("compose-show-bcc")
                                    .label("Bcc")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = state_entity.clone();
                                        move |_, _, app_cx| {
                                            set_compose_bcc_visible(&state_entity, true, app_cx);
                                        }
                                    }),
                            )
                        }),
                )
                .when(show_cc, |fields| {
                    fields.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(compose_field_label("Cc", cx))
                            .child(div().flex_1().min_w_0().child(cc_input.clone())),
                    )
                })
                .when(show_bcc, |fields| {
                    fields.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(compose_field_label("Bcc", cx))
                            .child(div().flex_1().min_w_0().child(bcc_input.clone())),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(compose_field_label("Subject", cx))
                        .child(div().flex_1().min_w_0().child(subject_input.clone())),
                ),
        )
        .child(body_section)
        .child(
            div()
                .border_t_1()
                .border_color(colors.stroke_neutral_subtle)
                .px(px(20.0))
                .py(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    Button::new("compose-send")
                        .label(if compose.sending {
                            "Sending..."
                        } else {
                            "Send"
                        })
                        .appearance(ButtonAppearance::Accent)
                        .size(ButtonSize::Compact)
                        .disabled(!can_send)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let to_input = to_input.clone();
                            let cc_input = cc_input.clone();
                            let bcc_input = bcc_input.clone();
                            let subject_input = subject_input.clone();
                            let body_input = body_input.clone();
                            move |_, _, app_cx| {
                                start_compose_send(
                                    &state_entity,
                                    to_input.clone(),
                                    cc_input.clone(),
                                    bcc_input.clone(),
                                    subject_input.clone(),
                                    body_input.clone(),
                                    app_cx,
                                );
                            }
                        }),
                )
                .child(
                    Button::new("compose-discard")
                        .label("Discard")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(compose.sending)
                        .on_click({
                            let state_entity = state_entity.clone();
                            move |_, _, app_cx| {
                                discard_compose(&state_entity, app_cx);
                            }
                        }),
                )
                .when_some(compose_status, |footer, (status, is_error)| {
                    footer.child(
                        div()
                            .text_size(px(typography.caption.size))
                            .text_color(if is_error {
                                colors.status_error
                            } else {
                                colors.on_subtle
                            })
                            .child(status),
                    )
                }),
        )
}

/// Display text for an identity: `Name <email>`, or just the address.
pub(crate) fn identity_label(identity: &Identity) -> String {
    let name = identity.name.trim();
    if name.is_empty() {
        identity.email.clone()
    } else {
        format!("{name} <{}>", identity.email)
    }
}

/// The identity the From row shows as chosen.
pub(crate) fn effective_identity_id(compose: &ComposeState) -> Option<String> {
    selected_send_identity(compose).or_else(|| {
        compose
            .identities
            .iter()
            .find(|identity| identity.is_default)
            .or_else(|| compose.identities.first())
            .map(|identity| identity.id.clone())
    })
}

pub(crate) fn compose_identity_row(
    state_entity: &Entity<NativeAppState>,
    compose: &ComposeState,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let current = effective_identity_id(compose);
    let mut choices = div().flex_1().min_w_0().flex().flex_wrap().gap(px(6.0));
    for identity in &compose.identities {
        let selected = current.as_deref() == Some(identity.id.as_str());
        let id = identity.id.clone();
        choices = choices.child(
            Button::new(SharedString::from(format!(
                "compose-identity-{}",
                identity.id
            )))
            .label(identity_label(identity))
            .appearance(if selected {
                ButtonAppearance::Accent
            } else {
                ButtonAppearance::Subtle
            })
            .size(ButtonSize::Compact)
            .disabled(compose.sending)
            .on_click({
                let state_entity = state_entity.clone();
                move |_, _, app_cx| set_compose_identity(&state_entity, id.clone(), app_cx)
            }),
        );
    }
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(compose_field_label("From", cx))
        .child(choices)
}

pub(crate) fn compose_mode_label(mode: &str) -> &'static str {
    match mode {
        "reply" => "Reply",
        "forward" => "Forward",
        _ => "New message",
    }
}

pub(crate) fn compose_field_label(
    label: &'static str,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    div()
        .w(px(58.0))
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

pub(crate) fn compose_signature_block(
    signature: &str,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let mut lines = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .text_size(px(typography.body.size))
        .line_height(px(typography.body.line_height))
        .text_color(colors.on_subtle);

    for line in signature.lines() {
        lines = lines.child(SharedString::from(if line.is_empty() {
            " ".to_string()
        } else {
            line.to_string()
        }));
    }

    div()
        .border_t_1()
        .border_color(colors.stroke_neutral_subtle)
        .pt(px(10.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child("--"),
        )
        .child(lines)
}

pub(crate) fn compose_quote_block(
    quote: &str,
    from: Option<&str>,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let heading = from
        .map(str::trim)
        .filter(|from| !from.is_empty())
        .map(|from| format!("{from} wrote:"))
        .unwrap_or_else(|| "Original message:".to_string());
    let mut lines = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .text_size(px(typography.caption.size))
        .line_height(px(typography.body.line_height))
        .text_color(colors.on_subtle);

    for line in quote.lines() {
        lines = lines.child(SharedString::from(if line.is_empty() {
            " ".to_string()
        } else {
            line.to_string()
        }));
    }

    div()
        .border_l_1()
        .border_color(colors.stroke_neutral)
        .pl(px(10.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_subtle)
                .child(heading),
        )
        .child(lines)
}

pub(crate) fn token_input_has_value(input: &Entity<TokenInput>, cx: &App) -> bool {
    input
        .read(cx)
        .values()
        .iter()
        .any(|token| token.valid && !token.value.trim().is_empty())
}

pub(crate) fn set_compose_cc_visible(
    state_entity: &Entity<NativeAppState>,
    visible: bool,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.compose.show_cc = visible;
        cx.notify();
    });
}

pub(crate) fn set_compose_bcc_visible(
    state_entity: &Entity<NativeAppState>,
    visible: bool,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.compose.show_bcc = visible;
        cx.notify();
    });
}
