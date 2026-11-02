use crate::prelude::*;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccountSetupMode {
    Choice,
    ManualImap,
    MicrosoftAppPassword,
}

impl AccountSetupMode {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Choice => "Connect account",
            Self::ManualImap => "Manual IMAP/SMTP",
            Self::MicrosoftAppPassword => "Microsoft app password",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccountSetupProvider {
    Gmail,
    Microsoft,
}

impl AccountSetupProvider {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Gmail => "Gmail",
            Self::Microsoft => "Microsoft 365",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccountSetupFormDraft {
    pub(crate) name: String,
    pub(crate) email: String,
    pub(crate) imap_host: String,
    pub(crate) imap_port: String,
    pub(crate) imap_password: String,
    pub(crate) smtp_host: String,
    pub(crate) smtp_port: String,
    pub(crate) smtp_password: String,
    pub(crate) same_password: bool,
}

pub(crate) struct AccountSetupModal {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) focused_on_open: bool,
    pub(crate) mode: AccountSetupMode,
    pub(crate) name_input: Entity<TextInput>,
    pub(crate) email_input: Entity<TextInput>,
    pub(crate) imap_host_input: Entity<TextInput>,
    pub(crate) imap_port_input: Entity<TextInput>,
    pub(crate) imap_password_input: Entity<TextInput>,
    pub(crate) smtp_host_input: Entity<TextInput>,
    pub(crate) smtp_port_input: Entity<TextInput>,
    pub(crate) smtp_password_input: Entity<TextInput>,
    pub(crate) same_password: bool,
    pub(crate) submitting: bool,
    pub(crate) refreshing: bool,
    pub(crate) oauth_loading: Option<AccountSetupProvider>,
    pub(crate) status: Option<(String, bool)>,
}

impl AccountSetupModal {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            focus_handle: cx.focus_handle().tab_stop(true),
            focused_on_open: false,
            mode: AccountSetupMode::Choice,
            name_input: cx.new(|_| TextInput::new().placeholder("Display name")),
            email_input: cx.new(|_| TextInput::new().placeholder("name@example.com")),
            imap_host_input: cx.new(|_| TextInput::new().placeholder("imap.example.com")),
            imap_port_input: cx.new(|_| TextInput::new().placeholder("993")),
            imap_password_input: cx
                .new(|_| TextInput::new().placeholder("IMAP password").masked(true)),
            smtp_host_input: cx.new(|_| TextInput::new().placeholder("smtp.example.com")),
            smtp_port_input: cx.new(|_| TextInput::new().placeholder("587")),
            smtp_password_input: cx
                .new(|_| TextInput::new().placeholder("SMTP password").masked(true)),
            same_password: true,
            submitting: false,
            refreshing: false,
            oauth_loading: None,
            status: None,
        }
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.submitting || self.refreshing || self.oauth_loading.is_some()
    }

    pub(crate) fn reset(&mut self, cx: &mut Context<Self>) {
        self.mode = AccountSetupMode::Choice;
        self.same_password = true;
        self.submitting = false;
        self.refreshing = false;
        self.oauth_loading = None;
        self.status = None;
        self.clear_inputs(cx);
        cx.notify();
    }

    pub(crate) fn clear_inputs(&mut self, cx: &mut Context<Self>) {
        self.name_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.email_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.imap_host_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.imap_port_input
            .update(cx, |input, cx| input.set_value("993", cx));
        self.imap_password_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.smtp_host_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.smtp_port_input
            .update(cx, |input, cx| input.set_value("587", cx));
        self.smtp_password_input
            .update(cx, |input, cx| input.set_value("", cx));
    }

    pub(crate) fn show_mode(&mut self, mode: AccountSetupMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.same_password = true;
        self.submitting = false;
        self.refreshing = false;
        self.oauth_loading = None;
        self.status = None;
        self.clear_inputs(cx);
        if mode == AccountSetupMode::MicrosoftAppPassword {
            self.imap_host_input.update(cx, |input, cx| {
                input.set_value("outlook.office365.com", cx);
            });
            self.smtp_host_input.update(cx, |input, cx| {
                input.set_value("smtp.office365.com", cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn set_status(&mut self, status: Option<(String, bool)>, cx: &mut Context<Self>) {
        self.status = status;
        self.submitting = false;
        self.refreshing = false;
        self.oauth_loading = None;
        cx.notify();
    }
}

impl Render for AccountSetupModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let first_run = state.accounts_resource.is_ready() && state.accounts.is_empty();
        if !first_run && !state.modals.account_setup_open {
            self.focused_on_open = false;
            return div();
        }
        if !self.focused_on_open {
            window.focus(&self.focus_handle);
            self.focused_on_open = true;
        }

        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let mut backdrop = colors.surface_dim;
        backdrop.a = 0.6;
        let modal_entity = cx.entity();
        let can_close = !first_run && !self.is_busy();

        div()
            .absolute()
            .inset_0()
            .size_full()
            .tab_index(0)
            .key_context("AccountSetupModal")
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .bg(backdrop)
            .on_key_down({
                let state_entity = self.state.clone();
                let modal_entity = modal_entity.clone();
                move |ev: &KeyDownEvent, _, app_cx| {
                    if handle_account_setup_key(
                        &state_entity,
                        &modal_entity,
                        ev.keystroke.key.as_str(),
                        app_cx,
                    ) {
                        app_cx.stop_propagation();
                    }
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let state_entity = self.state.clone();
                move |_, _, app_cx| {
                    if can_close {
                        close_account_setup_modal(&state_entity, app_cx);
                    }
                }
            })
            .child(
                div()
                    .w(px(620.0))
                    .max_h(px(680.0))
                    .flex()
                    .flex_col()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(colors.stroke_neutral)
                    .bg(colors.surface)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, app_cx| {
                        app_cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(18.0))
                            .py(px(14.0))
                            .border_b_1()
                            .border_color(colors.stroke_neutral_subtle)
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(typography.subtitle.size))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(colors.on_neutral)
                                            .child(if first_run {
                                                "Set up email-rs"
                                            } else {
                                                "Add account"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(typography.caption.size))
                                            .text_color(colors.on_subtle)
                                            .child(self.mode.title()),
                                    ),
                            )
                            .when(can_close, |header| {
                                header.child(
                                    Button::new("account-setup-close")
                                        .label("Close")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                close_account_setup_modal(&state_entity, app_cx);
                                            }
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("account-setup-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p(px(18.0))
                            .child(account_setup_body(
                                &self.state,
                                &modal_entity,
                                first_run,
                                self,
                                cx,
                            )),
                    ),
            )
    }
}

pub(crate) fn account_setup_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    first_run: bool,
    modal: &AccountSetupModal,
    cx: &mut Context<AccountSetupModal>,
) -> gpui::AnyElement {
    match modal.mode {
        AccountSetupMode::Choice => {
            account_setup_choice_body(state_entity, modal_entity, modal, cx).into_any_element()
        }
        AccountSetupMode::ManualImap | AccountSetupMode::MicrosoftAppPassword => {
            account_setup_form_body(state_entity, modal_entity, first_run, modal, cx)
                .into_any_element()
        }
    }
}

pub(crate) fn account_setup_choice_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    modal: &AccountSetupModal,
    cx: &mut Context<AccountSetupModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let busy = modal.is_busy();
    let gmail_loading = modal.oauth_loading == Some(AccountSetupProvider::Gmail);
    let microsoft_loading = modal.oauth_loading == Some(AccountSetupProvider::Microsoft);

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            Button::new("account-setup-gmail")
                .label(if gmail_loading {
                    "Opening Gmail..."
                } else {
                    "Connect Gmail"
                })
                .appearance(ButtonAppearance::Accent)
                .disabled(busy)
                .on_click({
                    let state_entity = state_entity.clone();
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        start_account_setup_oauth(
                            &state_entity,
                            &modal_entity,
                            AccountSetupProvider::Gmail,
                            app_cx,
                        );
                    }
                }),
        )
        .child(
            Button::new("account-setup-microsoft-oauth")
                .label(if microsoft_loading {
                    "Opening Microsoft 365..."
                } else {
                    "Connect Microsoft 365"
                })
                .appearance(ButtonAppearance::Neutral)
                .disabled(busy)
                .on_click({
                    let state_entity = state_entity.clone();
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        start_account_setup_oauth(
                            &state_entity,
                            &modal_entity,
                            AccountSetupProvider::Microsoft,
                            app_cx,
                        );
                    }
                }),
        )
        .child(
            div()
                .h(px(1.0))
                .my(px(4.0))
                .bg(colors.stroke_neutral_subtle),
        )
        .child(
            Button::new("account-setup-microsoft-app-password")
                .label("Use Microsoft app password")
                .appearance(ButtonAppearance::Subtle)
                .disabled(busy)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        modal_entity.update(app_cx, |modal, cx| {
                            modal.show_mode(AccountSetupMode::MicrosoftAppPassword, cx);
                        });
                    }
                }),
        )
        .child(
            Button::new("account-setup-manual")
                .label("Manual IMAP/SMTP")
                .appearance(ButtonAppearance::Subtle)
                .disabled(busy)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        modal_entity.update(app_cx, |modal, cx| {
                            modal.show_mode(AccountSetupMode::ManualImap, cx);
                        });
                    }
                }),
        )
        .when_some(modal.status.as_ref(), |body, status| {
            body.child(account_setup_status(status, cx)).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Button::new("account-setup-refresh")
                            .label(if modal.refreshing {
                                "Refreshing..."
                            } else {
                                "Refresh accounts"
                            })
                            .appearance(ButtonAppearance::Neutral)
                            .size(ButtonSize::Compact)
                            .disabled(busy)
                            .on_click({
                                let state_entity = state_entity.clone();
                                let modal_entity = modal_entity.clone();
                                move |_, _, app_cx| {
                                    start_account_setup_refresh(
                                        &state_entity,
                                        &modal_entity,
                                        app_cx,
                                    );
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(typography.caption.size))
                            .text_color(colors.on_subtle)
                            .child("Use after browser authorization completes."),
                    ),
            )
        })
}

pub(crate) fn account_setup_form_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    first_run: bool,
    modal: &AccountSetupModal,
    cx: &mut Context<AccountSetupModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let busy = modal.is_busy();
    let is_manual = modal.mode == AccountSetupMode::ManualImap;

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(account_setup_field("Name", modal.name_input.clone(), cx))
        .child(account_setup_field("Email", modal.email_input.clone(), cx))
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(div().flex_1().min_w_0().child(account_setup_field(
                    "IMAP host",
                    modal.imap_host_input.clone(),
                    cx,
                )))
                .child(div().w(px(120.0)).child(account_setup_field(
                    "Port",
                    modal.imap_port_input.clone(),
                    cx,
                ))),
        )
        .child(account_setup_field(
            if is_manual {
                "IMAP password"
            } else {
                "App password"
            },
            modal.imap_password_input.clone(),
            cx,
        ))
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(div().flex_1().min_w_0().child(account_setup_field(
                    "SMTP host",
                    modal.smtp_host_input.clone(),
                    cx,
                )))
                .child(div().w(px(120.0)).child(account_setup_field(
                    "Port",
                    modal.smtp_port_input.clone(),
                    cx,
                ))),
        )
        .when(is_manual, |form| {
            form.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Checkbox::new("account-setup-same-password")
                            .state(if modal.same_password {
                                CheckboxState::Checked
                            } else {
                                CheckboxState::Unchecked
                            })
                            .disabled(busy)
                            .on_click({
                                let modal_entity = modal_entity.clone();
                                move |new_state, _, _, app_cx| {
                                    let same_password = matches!(new_state, CheckboxState::Checked);
                                    modal_entity.update(app_cx, |modal, cx| {
                                        modal.same_password = same_password;
                                        modal.status = None;
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(typography.body.size))
                            .text_color(colors.on_neutral)
                            .child("Use IMAP password for SMTP"),
                    ),
            )
        })
        .when(is_manual && !modal.same_password, |form| {
            form.child(account_setup_field(
                "SMTP password",
                modal.smtp_password_input.clone(),
                cx,
            ))
        })
        .when_some(modal.status.as_ref(), |form, status| {
            form.child(account_setup_status(status, cx))
        })
        .child(
            div()
                .pt(px(4.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .child(
                    Button::new("account-setup-back")
                        .label(if first_run { "Back" } else { "Cancel" })
                        .appearance(ButtonAppearance::Subtle)
                        .disabled(busy)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                cancel_account_setup_form(
                                    &state_entity,
                                    &modal_entity,
                                    first_run,
                                    app_cx,
                                );
                            }
                        }),
                )
                .child(
                    Button::new("account-setup-submit")
                        .label(if modal.submitting {
                            "Adding..."
                        } else {
                            "Add account"
                        })
                        .appearance(ButtonAppearance::Accent)
                        .disabled(busy)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                start_account_setup_create(&state_entity, &modal_entity, app_cx);
                            }
                        }),
                ),
        )
}

pub(crate) fn account_setup_field(
    label: &'static str,
    input: Entity<TextInput>,
    cx: &mut Context<AccountSetupModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .text_size(px(theme.typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_subtle)
                .child(label),
        )
        .child(input)
}

pub(crate) fn account_setup_status(
    status: &(String, bool),
    cx: &mut Context<AccountSetupModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    div()
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if status.1 {
            colors.status_success
        } else {
            colors.status_error
        })
        .bg(colors.surface)
        .text_size(px(theme.typography.caption.size))
        .text_color(if status.1 {
            colors.status_success
        } else {
            colors.status_error
        })
        .child(status.0.clone())
}
