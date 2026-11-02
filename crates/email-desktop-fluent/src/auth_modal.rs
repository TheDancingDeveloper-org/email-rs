use crate::prelude::*;
use crate::*;

pub(crate) struct NativeAuthModal {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) username: Entity<TextInput>,
    pub(crate) password: Entity<TextInput>,
    pub(crate) setup: bool,
    pub(crate) busy: bool,
    pub(crate) error: Option<String>,
}

impl NativeAuthModal {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        Self {
            state,
            username: cx.new(|_| TextInput::new().placeholder("Username")),
            password: cx.new(|_| TextInput::new().placeholder("Password").masked(true)),
            setup: false,
            busy: false,
            error: None,
        }
    }
}

impl Render for NativeAuthModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.state.read(cx).auth_required {
            return div();
        }
        let colors = cx.theme().colors.clone();
        let modal = cx.entity();
        let state = self.state.clone();
        div()
            .absolute()
            .inset_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(colors.surface_dim)
            .child(
                div()
                    .w(px(420.0))
                    .p(px(24.0))
                    .rounded(px(8.0))
                    .bg(colors.surface)
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .child(if self.setup {
                                "Set up email-rs"
                            } else {
                                "Sign in to email-rs"
                            }),
                    )
                    .child(self.username.clone())
                    .child(self.password.clone())
                    .when_some(self.error.clone(), |card, error| {
                        card.child(div().text_color(colors.status_error).child(error))
                    })
                    .child(
                        Button::new("native-auth-submit")
                            .label(if self.busy {
                                "Working..."
                            } else if self.setup {
                                "Create user"
                            } else {
                                "Sign in"
                            })
                            .appearance(ButtonAppearance::Accent)
                            .disabled(self.busy)
                            .on_click(move |_, _, app_cx| {
                                start_native_auth(&modal, &state, app_cx)
                            }),
                    )
                    .child(
                        Button::new("native-auth-toggle")
                            .label(if self.setup {
                                "I already have a user"
                            } else {
                                "First-run setup"
                            })
                            .appearance(ButtonAppearance::Subtle)
                            .disabled(self.busy)
                            .on_click(cx.listener(|modal, _, _, cx| {
                                modal.setup = !modal.setup;
                                modal.error = None;
                                cx.notify();
                            })),
                    ),
            )
    }
}

pub(crate) fn start_native_auth(
    modal: &Entity<NativeAuthModal>,
    state: &Entity<NativeAppState>,
    app_cx: &mut App,
) {
    let (username, password, setup) = modal.update(app_cx, |modal, cx| {
        modal.busy = true;
        modal.error = None;
        cx.notify();
        (
            modal.username.read(cx).text().trim().to_string(),
            modal.password.read(cx).text().to_string(),
            modal.setup,
        )
    });
    if username.is_empty() || password.is_empty() {
        modal.update(app_cx, |modal, cx| {
            modal.busy = false;
            modal.error = Some("Username and password are required".into());
            cx.notify();
        });
        return;
    }
    let (base_url, accounts_request, messages_request) = state.update(app_cx, |state, _| {
        (
            state.api_base_url.clone(),
            state.accounts_resource.begin_load(),
            state.messages_resource.begin_load(),
        )
    });
    let task = app_cx.background_executor().spawn(async move {
        let client = EmailClient::new(&base_url)?;
        let session = if setup {
            client.bootstrap(&username, &password).await?
        } else {
            client.login(&username, &password).await?
        };
        let token = session
            .token
            .ok_or_else(|| anyhow!("server did not return a bearer token"))?;
        set_default_bearer_token(Some(token.clone()));
        save_native_auth_token(&token)?;
        let loaded = reload_accounts(base_url)?;
        Ok::<_, anyhow::Error>(loaded)
    });
    let modal_entity = modal.clone();
    let state_entity = state.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let _ = modal_entity.update(&mut async_cx, |modal, cx| {
                    modal.busy = false;
                    if let Err(error) = &result {
                        modal.error = Some(error.to_string());
                    }
                    cx.notify();
                });
                if let Ok(loaded) = result {
                    let _ = state_entity.update(&mut async_cx, |state, cx| {
                        state.auth_required = false;
                        apply_initial_account_load(
                            state,
                            accounts_request,
                            messages_request,
                            loaded,
                        );
                        cx.notify();
                    });
                }
            }
        })
        .detach();
}
