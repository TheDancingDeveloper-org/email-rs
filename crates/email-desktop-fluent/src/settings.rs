use crate::prelude::*;
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsTab {
    General,
    Account(String),
    Rules(String),
    Webhooks,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsRuleMutation {
    Save(SettingsRuleSaveRequest),
    Toggle { rule_id: String },
    Delete { rule_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SettingsRuleActionDraft {
    pub(crate) action_type: String,
    pub(crate) action_value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsRuleEditorMode {
    New,
    Edit(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SettingsRuleSaveRequest {
    pub(crate) mode: SettingsRuleEditorMode,
    pub(crate) account_id: String,
    pub(crate) name: String,
    pub(crate) match_mode: String,
    pub(crate) conditions: Vec<ConditionInput>,
    pub(crate) actions: Vec<ActionInput>,
}

#[derive(Clone)]
pub(crate) struct SettingsRuleConditionRow {
    pub(crate) key: u64,
    pub(crate) condition: Condition,
    pub(crate) value_input: Entity<TextInput>,
    pub(crate) field_dropdown: Entity<Dropdown>,
    pub(crate) operator_dropdown: Entity<Dropdown>,
    pub(crate) bool_dropdown: Entity<Dropdown>,
}

#[derive(Clone)]
pub(crate) struct SettingsRuleActionRow {
    pub(crate) key: u64,
    pub(crate) action_type: String,
    pub(crate) action_value: Option<String>,
    pub(crate) action_dropdown: Entity<Dropdown>,
    pub(crate) folder_dropdown: Entity<Dropdown>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsWebhookMutation {
    Create(CreateWebhook),
    Toggle { webhook_id: String, enabled: bool },
    Delete { webhook_id: String },
}

#[derive(Clone)]
pub(crate) struct SettingsModal {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) focused_on_open: bool,
    pub(crate) active_tab: SettingsTab,
    pub(crate) name_input: Entity<TextInput>,
    pub(crate) sync_days_input: Entity<TextInput>,
    pub(crate) signature_input: Entity<Textarea>,
    pub(crate) webhook_url_input: Entity<TextInput>,
    pub(crate) webhook_secret_input: Entity<TextInput>,
    pub(crate) webhook_events: String,
    pub(crate) webhooks: Vec<Webhook>,
    pub(crate) webhooks_loading: bool,
    pub(crate) webhook_busy_ids: BTreeSet<String>,
    pub(crate) adding_webhook: bool,
    pub(crate) rules_account_id: Option<String>,
    pub(crate) rules: Vec<RuleDto>,
    pub(crate) rules_loading: bool,
    pub(crate) rule_busy_ids: BTreeSet<String>,
    pub(crate) rule_editor_mode: Option<SettingsRuleEditorMode>,
    pub(crate) rule_editor_account_id: Option<String>,
    pub(crate) rule_name_input: Entity<TextInput>,
    pub(crate) rule_match_mode: MatchMode,
    pub(crate) rule_condition_rows: Vec<SettingsRuleConditionRow>,
    pub(crate) rule_action_rows: Vec<SettingsRuleActionRow>,
    pub(crate) rule_next_key: u64,
    pub(crate) rule_saving: bool,
    pub(crate) loading_account: bool,
    pub(crate) saving_account: bool,
    pub(crate) deleting_account: bool,
    pub(crate) confirm_delete_account: Option<String>,
    pub(crate) status: Option<(String, bool)>,
}

impl SettingsModal {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            focus_handle: cx.focus_handle().tab_stop(true),
            focused_on_open: false,
            active_tab: SettingsTab::General,
            name_input: cx.new(|_| TextInput::new().placeholder("Display name")),
            sync_days_input: cx.new(|_| TextInput::new().placeholder("All")),
            signature_input: cx.new(|_| {
                Textarea::new()
                    .placeholder("Your email signature...")
                    .rows(5)
            }),
            webhook_url_input: cx
                .new(|_| TextInput::new().placeholder("https://example.com/webhook")),
            webhook_secret_input: cx.new(|_| TextInput::new().placeholder("HMAC signing secret")),
            webhook_events: "new_message".to_string(),
            webhooks: Vec::new(),
            webhooks_loading: false,
            webhook_busy_ids: BTreeSet::new(),
            adding_webhook: false,
            rules_account_id: None,
            rules: Vec::new(),
            rules_loading: false,
            rule_busy_ids: BTreeSet::new(),
            rule_editor_mode: None,
            rule_editor_account_id: None,
            rule_name_input: cx.new(|_| TextInput::new().placeholder("Rule name")),
            rule_match_mode: MatchMode::All,
            rule_condition_rows: Vec::new(),
            rule_action_rows: Vec::new(),
            rule_next_key: 1,
            rule_saving: false,
            loading_account: false,
            saving_account: false,
            deleting_account: false,
            confirm_delete_account: None,
            status: None,
        }
    }

    pub(crate) fn reset(&mut self, cx: &mut Context<Self>) {
        self.active_tab = SettingsTab::General;
        self.loading_account = false;
        self.saving_account = false;
        self.deleting_account = false;
        self.webhooks_loading = false;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.rules_account_id = None;
        self.rules_loading = false;
        self.rule_busy_ids.clear();
        self.reset_rule_editor(cx);
        self.confirm_delete_account = None;
        self.status = None;
        self.name_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.sync_days_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.signature_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.webhook_url_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.webhook_secret_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.webhook_events = "new_message".to_string();
        cx.notify();
    }

    pub(crate) fn select_account(&mut self, account: &Account, cx: &mut Context<Self>) {
        self.active_tab = SettingsTab::Account(account.id.clone());
        self.loading_account = true;
        self.saving_account = false;
        self.deleting_account = false;
        self.confirm_delete_account = None;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.rules_loading = false;
        self.rule_busy_ids.clear();
        self.reset_rule_editor(cx);
        self.status = None;
        self.name_input
            .update(cx, |input, cx| input.set_value(account.name.clone(), cx));
        self.sync_days_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.signature_input
            .update(cx, |input, cx| input.set_value("", cx));
        cx.notify();
    }

    pub(crate) fn apply_account_settings(
        &mut self,
        settings: AccountSettings,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab != SettingsTab::Account(settings.id.clone()) {
            return;
        }

        self.loading_account = false;
        self.status = None;
        self.name_input
            .update(cx, |input, cx| input.set_value(settings.name, cx));
        self.sync_days_input.update(cx, |input, cx| {
            input.set_value(
                settings
                    .sync_days_limit
                    .map(|days| days.to_string())
                    .unwrap_or_default(),
                cx,
            );
        });
        self.signature_input.update(cx, |input, cx| {
            input.set_value(settings.signature.unwrap_or_default(), cx);
        });
        cx.notify();
    }

    pub(crate) fn fail_account_settings(
        &mut self,
        account_id: &str,
        message: String,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab != SettingsTab::Account(account_id.to_string()) {
            return;
        }
        self.loading_account = false;
        self.status = Some((message, false));
        cx.notify();
    }

    pub(crate) fn set_status(&mut self, status: Option<(String, bool)>, cx: &mut Context<Self>) {
        self.status = status;
        self.saving_account = false;
        self.deleting_account = false;
        cx.notify();
    }

    pub(crate) fn select_webhooks(&mut self, cx: &mut Context<Self>) {
        self.active_tab = SettingsTab::Webhooks;
        self.webhooks_loading = true;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.status = None;
        cx.notify();
    }

    pub(crate) fn select_rules(&mut self, account_id: String, cx: &mut Context<Self>) {
        self.active_tab = SettingsTab::Rules(account_id.clone());
        self.rules_account_id = Some(account_id);
        self.rules_loading = true;
        self.rule_busy_ids.clear();
        self.reset_rule_editor(cx);
        self.status = None;
        cx.notify();
    }

    pub(crate) fn apply_rules(
        &mut self,
        account_id: &str,
        rules: Vec<RuleDto>,
        status: Option<(String, bool)>,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab != SettingsTab::Rules(account_id.to_string()) {
            return;
        }
        self.rules_account_id = Some(account_id.to_string());
        self.rules = rules;
        self.rules_loading = false;
        self.rule_busy_ids.clear();
        self.status = status;
        cx.notify();
    }

    pub(crate) fn fail_rules(&mut self, account_id: &str, message: String, cx: &mut Context<Self>) {
        if self.active_tab != SettingsTab::Rules(account_id.to_string()) {
            return;
        }
        self.rules_loading = false;
        self.rule_busy_ids.clear();
        self.rule_saving = false;
        self.status = Some((message, false));
        cx.notify();
    }

    pub(crate) fn reset_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.rule_editor_mode = None;
        self.rule_editor_account_id = None;
        self.rule_match_mode = MatchMode::All;
        self.rule_condition_rows.clear();
        self.rule_action_rows.clear();
        self.rule_next_key = 1;
        self.rule_saving = false;
        self.rule_name_input
            .update(cx, |input, cx| input.set_value("", cx));
    }

    pub(crate) fn start_new_rule_editor(
        &mut self,
        account_id: String,
        folders: Vec<Folder>,
        cx: &mut Context<Self>,
    ) {
        self.reset_rule_editor(cx);
        self.rule_editor_mode = Some(SettingsRuleEditorMode::New);
        self.rule_editor_account_id = Some(account_id);
        self.rule_match_mode = MatchMode::All;
        let condition_id = self.next_rule_condition_id();
        self.add_rule_condition(new_advanced_condition(condition_id), cx);
        self.add_rule_action("mark_read".to_string(), None, folders, cx);
        self.status = None;
        cx.notify();
    }

    pub(crate) fn start_edit_rule_editor(
        &mut self,
        account_id: String,
        rule: RuleDto,
        folders: Vec<Folder>,
        cx: &mut Context<Self>,
    ) {
        self.reset_rule_editor(cx);
        self.rule_editor_mode = Some(SettingsRuleEditorMode::Edit(rule.id));
        self.rule_editor_account_id = Some(account_id);
        self.rule_name_input
            .update(cx, |input, cx| input.set_value(rule.name, cx));
        self.rule_match_mode = match rule.match_mode.as_str() {
            "any" => MatchMode::Any,
            _ => MatchMode::All,
        };

        for condition in rule.conditions {
            let field = parse_condition_field(&condition.field).unwrap_or(ConditionField::From);
            let operator = parse_condition_operator(&condition.operator)
                .unwrap_or(ConditionOperator::Contains);
            self.add_rule_condition(
                Condition {
                    id: condition.id,
                    field,
                    operator,
                    value: condition.value,
                },
                cx,
            );
        }
        if self.rule_condition_rows.is_empty() {
            let condition_id = self.next_rule_condition_id();
            self.add_rule_condition(new_advanced_condition(condition_id), cx);
        }

        for action in rule.actions {
            self.add_rule_action(action.action_type, action.action_value, folders.clone(), cx);
        }
        if self.rule_action_rows.is_empty() {
            self.add_rule_action("mark_read".to_string(), None, folders, cx);
        }
        self.status = None;
        cx.notify();
    }

    pub(crate) fn next_rule_key(&mut self) -> u64 {
        let key = self.rule_next_key;
        self.rule_next_key = self.rule_next_key.saturating_add(1);
        key
    }

    pub(crate) fn next_rule_condition_id(&mut self) -> String {
        format!("settings-rule-condition-{}", self.next_rule_key())
    }

    pub(crate) fn add_rule_condition(&mut self, condition: Condition, cx: &mut Context<Self>) {
        let key = self.next_rule_key();
        let condition_id = condition.id.clone();
        let initial_value = condition.value.clone();
        let modal_entity = cx.entity();
        let value_input = cx.new(move |_| {
            TextInput::new()
                .value(initial_value.clone())
                .placeholder("Value")
                .on_change({
                    let modal_entity = modal_entity.clone();
                    let condition_id = condition_id.clone();
                    move |value, app_cx| {
                        modal_entity.update(app_cx, |modal, cx| {
                            modal.set_rule_condition_value(&condition_id, value.to_string());
                            cx.notify();
                        });
                    }
                })
        });
        let field_condition_id = condition.id.clone();
        let field_modal = cx.entity();
        let field_selected = condition_field_value(&condition.field);
        let field_dropdown = cx.new(move |_| {
            rule_condition_field_dropdown(
                ("settings-rule-field", key),
                field_selected,
                field_modal.clone(),
                field_condition_id.clone(),
            )
        });
        let operator_condition_id = condition.id.clone();
        let operator_modal = cx.entity();
        let operator_selected = condition_operator_value(&condition.operator);
        let operator_dropdown = cx.new(move |_| {
            rule_condition_operator_dropdown(
                ("settings-rule-op", key),
                operator_selected,
                operator_modal.clone(),
                operator_condition_id.clone(),
            )
        });
        let bool_condition_id = condition.id.clone();
        let bool_modal = cx.entity();
        let bool_selected = if condition.value.trim() == "false" {
            "false"
        } else {
            "true"
        };
        let bool_dropdown = cx.new(move |_| {
            rule_boolean_dropdown(
                ("settings-rule-bool", key),
                bool_selected,
                bool_modal.clone(),
                bool_condition_id.clone(),
            )
        });
        self.rule_condition_rows.push(SettingsRuleConditionRow {
            key,
            condition,
            value_input,
            field_dropdown,
            operator_dropdown,
            bool_dropdown,
        });
    }

    pub(crate) fn remove_rule_condition(&mut self, condition_id: &str, cx: &mut Context<Self>) {
        self.rule_condition_rows
            .retain(|row| row.condition.id != condition_id);
        if self.rule_condition_rows.is_empty() {
            let id = self.next_rule_condition_id();
            self.add_rule_condition(new_advanced_condition(id), cx);
        }
        cx.notify();
    }

    pub(crate) fn set_rule_condition_field(
        &mut self,
        condition_id: &str,
        field: ConditionField,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self
            .rule_condition_rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.field = field;
            row.condition.operator = ConditionOperator::Contains;
            row.condition.value = default_condition_value(&row.condition.field).to_string();
            let value = row.condition.value.clone();
            let operator = row.condition.operator.clone();
            let bool_value = if row.condition.value == "false" {
                "false"
            } else {
                "true"
            };
            row.value_input.update(cx, |input, cx| {
                input.set_value(value, cx);
            });
            row.operator_dropdown.update(cx, |dropdown, cx| {
                dropdown.set_selected(condition_operator_value(&operator), cx);
            });
            row.bool_dropdown.update(cx, |dropdown, cx| {
                dropdown.set_selected(bool_value, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn set_rule_condition_operator(
        &mut self,
        condition_id: &str,
        operator: ConditionOperator,
    ) {
        if let Some(row) = self
            .rule_condition_rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.operator = operator;
        }
    }

    pub(crate) fn set_rule_condition_value(&mut self, condition_id: &str, value: String) {
        if let Some(row) = self
            .rule_condition_rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.value = value;
        }
    }

    pub(crate) fn add_rule_action(
        &mut self,
        action_type: String,
        action_value: Option<String>,
        folders: Vec<Folder>,
        cx: &mut Context<Self>,
    ) {
        let key = self.next_rule_key();
        let modal_entity = cx.entity();
        let action_dropdown = cx.new({
            let action_type = action_type.clone();
            let modal_entity = modal_entity.clone();
            move |_| {
                rule_action_type_dropdown(
                    ("settings-rule-action-type", key),
                    action_type.as_str(),
                    modal_entity.clone(),
                    key,
                )
            }
        });
        let folder_dropdown = cx.new({
            let selected = action_value.clone();
            let modal_entity = cx.entity();
            move |_| {
                rule_action_folder_dropdown(
                    ("settings-rule-action-folder", key),
                    selected.clone(),
                    folders.clone(),
                    modal_entity.clone(),
                    key,
                )
            }
        });
        self.rule_action_rows.push(SettingsRuleActionRow {
            key,
            action_type,
            action_value,
            action_dropdown,
            folder_dropdown,
        });
        cx.notify();
    }

    pub(crate) fn remove_rule_action(&mut self, key: u64, cx: &mut Context<Self>) {
        if self.rule_action_rows.len() <= 1 {
            cx.notify();
            return;
        }
        self.rule_action_rows.retain(|row| row.key != key);
        cx.notify();
    }

    pub(crate) fn set_rule_action_type(&mut self, key: u64, action_type: String) {
        if let Some(row) = self.rule_action_rows.iter_mut().find(|row| row.key == key) {
            row.action_type = action_type;
            if row.action_type != "move_to_folder" {
                row.action_value = None;
            }
        }
    }

    pub(crate) fn set_rule_action_value(&mut self, key: u64, action_value: Option<String>) {
        if let Some(row) = self.rule_action_rows.iter_mut().find(|row| row.key == key) {
            row.action_value = action_value;
        }
    }

    pub(crate) fn set_rule_match_mode(&mut self, match_mode: MatchMode) {
        self.rule_match_mode = match_mode;
    }

    pub(crate) fn apply_webhooks(
        &mut self,
        webhooks: Vec<Webhook>,
        status: Option<(String, bool)>,
        clear_new: bool,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab != SettingsTab::Webhooks {
            return;
        }
        self.webhooks = webhooks;
        self.webhooks_loading = false;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.status = status;
        if clear_new {
            self.webhook_url_input
                .update(cx, |input, cx| input.set_value("", cx));
            self.webhook_secret_input
                .update(cx, |input, cx| input.set_value("", cx));
            self.webhook_events = "new_message".to_string();
        }
        cx.notify();
    }

    pub(crate) fn fail_webhooks(&mut self, message: String, cx: &mut Context<Self>) {
        if self.active_tab != SettingsTab::Webhooks {
            return;
        }
        self.webhooks_loading = false;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.status = Some((message, false));
        cx.notify();
    }
}

impl Render for SettingsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        if !state.modals.settings_open {
            self.focused_on_open = false;
            return div();
        }
        if !self.focused_on_open {
            window.focus(&self.focus_handle);
            self.focused_on_open = true;
        }

        let theme = cx.theme();
        let colors = theme.colors.clone();
        let mut backdrop = colors.surface_dim;
        backdrop.a = 0.58;
        let modal_entity = cx.entity();

        div()
            .absolute()
            .inset_0()
            .size_full()
            .tab_index(0)
            .key_context("SettingsModal")
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .bg(backdrop)
            .on_key_down({
                let state_entity = self.state.clone();
                let modal_entity = modal_entity.clone();
                move |ev: &KeyDownEvent, _, app_cx| {
                    if handle_settings_modal_key(
                        &state_entity,
                        &modal_entity,
                        ev.keystroke.key.as_str(),
                        ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform,
                        app_cx,
                    ) {
                        app_cx.stop_propagation();
                    }
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let state_entity = self.state.clone();
                move |_, _, app_cx| close_settings_modal(&state_entity, app_cx)
            })
            .child(
                div()
                    .w(px(860.0))
                    .max_h(px(620.0))
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
                    .child(settings_header(&self.state, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .child(settings_tabs(&self.state, &modal_entity, &state, self, cx))
                            .child(settings_body(&self.state, &modal_entity, &state, self, cx)),
                    ),
            )
    }
}

pub(crate) fn settings_header(
    state_entity: &Entity<NativeAppState>,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    div()
        .px(px(18.0))
        .py(px(14.0))
        .border_b_1()
        .border_color(colors.stroke_neutral_subtle)
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(typography.subtitle.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_neutral)
                .child("Settings"),
        )
        .child(
            Button::new("settings-close")
                .label("Close")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .on_click({
                    let state_entity = state_entity.clone();
                    move |_, _, app_cx| close_settings_modal(&state_entity, app_cx)
                }),
        )
}

pub(crate) fn settings_tabs(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    state: &NativeAppState,
    modal: &SettingsModal,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mut tabs = div()
        .w(px(220.0))
        .min_h_0()
        .border_r_1()
        .border_color(colors.stroke_neutral_subtle)
        .p(px(10.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(settings_tab_button(
            "settings-tab-general",
            "General",
            modal.active_tab == SettingsTab::General,
            {
                let modal_entity = modal_entity.clone();
                move |_, _, app_cx| {
                    modal_entity.update(app_cx, |modal, cx| {
                        modal.active_tab = SettingsTab::General;
                        modal.status = None;
                        cx.notify();
                    });
                }
            },
            cx,
        ));

    for account in &state.accounts {
        tabs = tabs.child(settings_tab_button(
            SharedString::from(format!("settings-tab-account-{}", account.id)),
            account.name.as_str(),
            modal.active_tab == SettingsTab::Account(account.id.clone()),
            {
                let state_entity = state_entity.clone();
                let modal_entity = modal_entity.clone();
                let account_id = account.id.clone();
                move |_, _, app_cx| {
                    select_settings_account(
                        &state_entity,
                        &modal_entity,
                        account_id.clone(),
                        app_cx,
                    );
                }
            },
            cx,
        ));
    }

    for account in &state.accounts {
        let label = format!("Rules ({})", account.name);
        tabs = tabs.child(settings_tab_button(
            SharedString::from(format!("settings-tab-rules-{}", account.id)),
            label.as_str(),
            modal.active_tab == SettingsTab::Rules(account.id.clone()),
            {
                let state_entity = state_entity.clone();
                let modal_entity = modal_entity.clone();
                let account_id = account.id.clone();
                move |_, _, app_cx| {
                    select_settings_rules(&state_entity, &modal_entity, account_id.clone(), app_cx);
                }
            },
            cx,
        ));
    }

    tabs.child(settings_tab_button(
        "settings-tab-webhooks",
        "Webhooks",
        modal.active_tab == SettingsTab::Webhooks,
        {
            let state_entity = state_entity.clone();
            let modal_entity = modal_entity.clone();
            move |_, _, app_cx| {
                select_settings_webhooks(&state_entity, &modal_entity, app_cx);
            }
        },
        cx,
    ))
}

pub(crate) fn settings_tab_button(
    id: impl Into<gpui::ElementId>,
    label: &str,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<SettingsModal>,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    div()
        .id(id)
        .h(px(32.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(if selected {
            colors.neutral_selected
        } else {
            colors.surface
        })
        .text_size(px(theme.typography.body.size))
        .font_weight(if selected {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(colors.on_neutral)
        .cursor_pointer()
        .on_click(on_click)
        .child(label.to_string())
        .into_any_element()
}

pub(crate) fn settings_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    state: &NativeAppState,
    modal: &SettingsModal,
    cx: &mut Context<SettingsModal>,
) -> gpui::AnyElement {
    match &modal.active_tab {
        SettingsTab::General => div()
            .id("settings-body-general")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(18.0))
            .child(settings_general_body(state_entity, state, cx))
            .into_any_element(),
        SettingsTab::Account(account_id) => div()
            .id("settings-body-account")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(18.0))
            .child(settings_account_body(
                state_entity,
                modal_entity,
                state,
                modal,
                account_id,
                cx,
            ))
            .into_any_element(),
        SettingsTab::Rules(account_id) => div()
            .id("settings-body-rules")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(18.0))
            .child(settings_rules_body(
                state,
                modal_entity,
                modal,
                account_id,
                cx,
            ))
            .into_any_element(),
        SettingsTab::Webhooks => div()
            .id("settings-body-webhooks")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(18.0))
            .child(settings_webhooks_body(modal_entity, modal, cx))
            .into_any_element(),
    }
}

pub(crate) fn settings_general_body(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let theme_is_dark = state.theme == ThemeMode::Dark;
    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(settings_section_title("Appearance", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Theme", cx))
                .child(
                    Button::new("settings-theme-light")
                        .label("Light")
                        .appearance(if theme_is_dark {
                            ButtonAppearance::Subtle
                        } else {
                            ButtonAppearance::Neutral
                        })
                        .size(ButtonSize::Compact)
                        .on_click(|_, _, app_cx| app_cx.dispatch_action(&ApplyLightTheme)),
                )
                .child(
                    Button::new("settings-theme-dark")
                        .label("Dark")
                        .appearance(if theme_is_dark {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click(|_, _, app_cx| app_cx.dispatch_action(&ApplyDarkTheme)),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Density", cx))
                .child(
                    IconButton::new("settings-density-down", "icons/chevron_down.svg")
                        .size(ButtonSize::Compact)
                        .disabled(state.density.level == 0)
                        .on_click(|_, _, app_cx| app_cx.dispatch_action(&DecreaseDensity)),
                )
                .child(
                    div()
                        .w(px(96.0))
                        .text_size(px(typography.body.size))
                        .text_color(colors.on_neutral)
                        .child(format!("Level {}", state.density.level)),
                )
                .child(
                    IconButton::new("settings-density-up", "icons/chevron_up.svg")
                        .size(ButtonSize::Compact)
                        .disabled(state.density.level >= 8)
                        .on_click(|_, _, app_cx| app_cx.dispatch_action(&IncreaseDensity)),
                ),
        )
        .child(settings_section_title("Session", cx))
        .child(
            Button::new("settings-logout")
                .label("Log out")
                .appearance(ButtonAppearance::Subtle)
                .on_click({
                    let state_entity = state_entity.clone();
                    move |_, _, app_cx| start_native_logout(&state_entity, app_cx)
                }),
        )
}

pub(crate) fn start_native_logout(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let task = app_cx.background_executor().spawn(async move {
        let _ = EmailClient::new(base_url)?.logout().await;
        Ok::<_, anyhow::Error>(())
    });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let _ = task.await;
                clear_native_auth_token();
                let _ = state_entity.update(&mut async_cx, |state, cx| {
                    state.auth_required = true;
                    state.accounts.clear();
                    state.messages.clear();
                    state.modals.settings_open = false;
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn settings_account_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    state: &NativeAppState,
    modal: &SettingsModal,
    account_id: &str,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let Some(account) = state
        .accounts
        .iter()
        .find(|account| account.id == account_id)
    else {
        return div()
            .text_size(px(typography.body.size))
            .text_color(colors.on_subtle)
            .child("Account not found.");
    };
    let folders = state
        .folders_by_account
        .get(account_id)
        .cloned()
        .unwrap_or_default();
    let status = modal.status.clone();
    let confirming_delete = modal.confirm_delete_account.as_deref() == Some(account_id);
    let account_email = account.email.clone();

    let mut folder_list = div().flex().flex_col().gap(px(4.0));
    if folders.is_empty() {
        folder_list = folder_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("No folders synced yet."),
        );
    } else {
        for folder in folders {
            let folder_id = folder.id.clone();
            let next_excluded = !folder.is_excluded;
            folder_list = folder_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Checkbox::new(SharedString::from(format!("settings-folder-{}", folder.id)))
                            .state(if folder.is_excluded {
                                CheckboxState::Unchecked
                            } else {
                                CheckboxState::Checked
                            })
                            .on_click({
                                let state_entity = state_entity.clone();
                                let modal_entity = modal_entity.clone();
                                move |_, _, _, app_cx| {
                                    start_folder_exclusion_toggle(
                                        &state_entity,
                                        &modal_entity,
                                        folder_id.clone(),
                                        next_excluded,
                                        app_cx,
                                    );
                                }
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(typography.body.size))
                            .text_color(colors.on_neutral)
                            .child(folder_label(&folder)),
                    )
                    .when(folder.is_excluded, |row| {
                        row.child(
                            div()
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child("excluded"),
                        )
                    }),
            );
        }
    }

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(settings_section_title("Account info", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Display name", cx))
                .child(div().w(px(260.0)).child(modal.name_input.clone())),
        )
        .child(settings_readonly_row("Email", account.email.as_str(), cx))
        .child(settings_readonly_row(
            "Provider",
            account.provider_type.as_str(),
            cx,
        ))
        .child(settings_section_title("Sync settings", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Download days", cx))
                .child(div().w(px(140.0)).child(modal.sync_days_input.clone()))
                .child(
                    div()
                        .text_size(px(typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child("Leave empty for all"),
                ),
        )
        .child(settings_section_title("Signature", cx))
        .child(modal.signature_input.clone())
        .child(settings_section_title("Excluded folders", cx))
        .child(folder_list)
        .when(modal.loading_account, |body| {
            body.child(
                div()
                    .text_size(px(typography.caption.size))
                    .text_color(colors.on_subtle)
                    .child("Loading account settings..."),
            )
        })
        .when_some(status, |body, (message, ok)| {
            body.child(
                div()
                    .text_size(px(typography.caption.size))
                    .text_color(if ok {
                        colors.status_success
                    } else {
                        colors.status_error
                    })
                    .child(message),
            )
        })
        .child(
            div().flex().items_center().gap(px(8.0)).child(
                Button::new("settings-save-account")
                    .label(if modal.saving_account {
                        "Saving..."
                    } else {
                        "Save changes"
                    })
                    .appearance(ButtonAppearance::Accent)
                    .size(ButtonSize::Compact)
                    .disabled(modal.loading_account || modal.saving_account)
                    .on_click({
                        let state_entity = state_entity.clone();
                        let modal_entity = modal_entity.clone();
                        let account_id = account_id.to_string();
                        move |_, _, app_cx| {
                            start_settings_save(
                                &state_entity,
                                &modal_entity,
                                account_id.clone(),
                                app_cx,
                            );
                        }
                    }),
            ),
        )
        .child(settings_section_title("Danger zone", cx))
        .child(if confirming_delete {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(typography.body.size))
                        .text_color(colors.on_neutral)
                        .child(format!("Delete {account_email} and all its cached emails?")),
                )
                .child(
                    Button::new("settings-confirm-delete-account")
                        .label(if modal.deleting_account {
                            "Deleting..."
                        } else {
                            "Yes, delete"
                        })
                        .appearance(ButtonAppearance::Danger)
                        .size(ButtonSize::Compact)
                        .disabled(
                            modal.loading_account || modal.saving_account || modal.deleting_account,
                        )
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let account_id = account_id.to_string();
                            move |_, _, app_cx| {
                                start_settings_account_delete(
                                    &state_entity,
                                    &modal_entity,
                                    account_id.clone(),
                                    app_cx,
                                );
                            }
                        }),
                )
                .child(
                    Button::new("settings-cancel-delete-account")
                        .label("Cancel")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(modal.deleting_account)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                cancel_settings_account_delete(&modal_entity, app_cx);
                            }
                        }),
                )
                .into_any_element()
        } else {
            Button::new("settings-remove-account")
                .label("Remove account")
                .appearance(ButtonAppearance::Danger)
                .size(ButtonSize::Compact)
                .disabled(modal.loading_account || modal.saving_account || modal.deleting_account)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    let account_id = account_id.to_string();
                    move |_, _, app_cx| {
                        confirm_settings_account_delete(&modal_entity, account_id.clone(), app_cx);
                    }
                })
                .into_any_element()
        })
}

pub(crate) fn settings_rules_body(
    state: &NativeAppState,
    modal_entity: &Entity<SettingsModal>,
    modal: &SettingsModal,
    account_id: &str,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let status = modal.status.clone();
    let state_entity = modal.state.clone();
    let account_label = state
        .accounts
        .iter()
        .find(|account| account.id == account_id)
        .map(|account| account.name.clone())
        .unwrap_or_else(|| "Account".to_string());
    let folders = state
        .folders_by_account
        .get(account_id)
        .cloned()
        .unwrap_or_default();
    let editor_open = modal.rule_editor_mode.is_some()
        && modal.rule_editor_account_id.as_deref() == Some(account_id);

    let mut rule_list = div().flex().flex_col().gap(px(6.0));
    if modal.rules_loading {
        rule_list = rule_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("Loading rules..."),
        );
    } else if modal.rules_account_id.as_deref() != Some(account_id) {
        rule_list = rule_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("Select this tab to load rules."),
        );
    } else if modal.rules.is_empty() {
        rule_list = rule_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("No rules configured for this account."),
        );
    } else {
        for rule in &modal.rules {
            let rule_id = rule.id.clone();
            let edit_rule = rule.clone();
            let busy = modal.rule_busy_ids.contains(&rule.id);
            rule_list = rule_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .child(
                        Checkbox::new(SharedString::from(format!(
                            "settings-rule-active-{}",
                            rule.id
                        )))
                        .checked(rule.is_active)
                        .disabled(busy || modal.rules_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let account_id = account_id.to_string();
                            let rule_id = rule_id.clone();
                            move |_, _, _, app_cx| {
                                start_settings_rule_toggle(
                                    &state_entity,
                                    &modal_entity,
                                    account_id.clone(),
                                    rule_id.clone(),
                                    app_cx,
                                );
                            }
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.body.size))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.on_neutral)
                                    .child(rule.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(typography.caption.size))
                                    .text_color(colors.on_subtle)
                                    .child(rule_counts_text(rule)),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-rule-edit-{}",
                            rule.id
                        )))
                        .label("Edit")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(busy || modal.rules_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let account_id = account_id.to_string();
                            let edit_rule = edit_rule.clone();
                            move |_, _, app_cx| {
                                start_settings_rule_edit(
                                    &state_entity,
                                    &modal_entity,
                                    account_id.clone(),
                                    edit_rule.clone(),
                                    app_cx,
                                );
                            }
                        }),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-rule-delete-{}",
                            rule.id
                        )))
                        .label("Delete")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(busy || modal.rules_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let account_id = account_id.to_string();
                            let rule_id = rule_id.clone();
                            move |_, _, app_cx| {
                                start_settings_rule_delete(
                                    &state_entity,
                                    &modal_entity,
                                    account_id.clone(),
                                    rule_id.clone(),
                                    app_cx,
                                );
                            }
                        }),
                    ),
            );
        }
    }

    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(settings_section_title("Rules", cx))
                .when(!editor_open, |row| {
                    row.child(
                        Button::new("settings-new-rule")
                            .label("New rule")
                            .appearance(ButtonAppearance::Accent)
                            .size(ButtonSize::Compact)
                            .disabled(modal.rules_loading)
                            .on_click({
                                let state_entity = state_entity.clone();
                                let modal_entity = modal_entity.clone();
                                let account_id = account_id.to_string();
                                move |_, _, app_cx| {
                                    start_settings_rule_new(
                                        &state_entity,
                                        &modal_entity,
                                        account_id.clone(),
                                        app_cx,
                                    );
                                }
                            }),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child(account_label),
        );

    if editor_open {
        body = body.child(settings_rule_editor_body(modal_entity, modal, folders, cx));
    } else {
        body = body.child(rule_list);
    }

    body.when_some(status, |body, (message, ok)| {
        body.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(if ok {
                    colors.status_success
                } else {
                    colors.status_error
                })
                .child(message),
        )
    })
}

pub(crate) fn settings_rule_editor_body(
    modal_entity: &Entity<SettingsModal>,
    modal: &SettingsModal,
    folders: Vec<Folder>,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let state_entity = modal.state.clone();
    let modal_entity_for_rows = modal_entity.clone();
    let title = match modal.rule_editor_mode {
        Some(SettingsRuleEditorMode::New) => "New rule",
        Some(SettingsRuleEditorMode::Edit(_)) => "Edit rule",
        None => "Rule",
    };

    let mut condition_rows = div().flex().flex_col().gap(px(8.0));
    for row in modal.rule_condition_rows.clone() {
        condition_rows = condition_rows.child(settings_rule_condition_row(
            row,
            modal_entity_for_rows.clone(),
            cx,
        ));
    }

    let mut action_rows = div().flex().flex_col().gap(px(8.0));
    let can_remove_action = modal.rule_action_rows.len() > 1;
    for row in modal.rule_action_rows.clone() {
        action_rows = action_rows.child(settings_rule_action_row(
            row,
            modal_entity.clone(),
            can_remove_action,
            cx,
        ));
    }

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(typography.body.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_neutral)
                .child(title),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Name", cx))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(240.0))
                        .child(modal.rule_name_input.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Match", cx))
                .child(
                    Button::new("settings-rule-match-all")
                        .label("All")
                        .appearance(if modal.rule_match_mode == MatchMode::All {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                modal_entity.update(app_cx, |modal, cx| {
                                    modal.set_rule_match_mode(MatchMode::All);
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    Button::new("settings-rule-match-any")
                        .label("Any")
                        .appearance(if modal.rule_match_mode == MatchMode::Any {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                modal_entity.update(app_cx, |modal, cx| {
                                    modal.set_rule_match_mode(MatchMode::Any);
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    div()
                        .text_size(px(typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child("of the following conditions"),
                ),
        )
        .child(settings_section_title("Conditions", cx))
        .child(condition_rows)
        .child(
            Button::new("settings-rule-add-condition")
                .label("Add condition")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(modal.rule_saving)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        modal_entity.update(app_cx, |modal, cx| {
                            let id = modal.next_rule_condition_id();
                            modal.add_rule_condition(new_advanced_condition(id), cx);
                        });
                    }
                }),
        )
        .child(settings_section_title("Actions", cx))
        .child(action_rows)
        .child(
            Button::new("settings-rule-add-action")
                .label("Add action")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(modal.rule_saving)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    let folders = folders.clone();
                    move |_, _, app_cx| {
                        modal_entity.update(app_cx, |modal, cx| {
                            modal.add_rule_action(
                                "mark_read".to_string(),
                                None,
                                folders.clone(),
                                cx,
                            );
                        });
                    }
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_1())
                .child(
                    Button::new("settings-rule-cancel")
                        .label("Cancel")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(modal.rule_saving)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                cancel_settings_rule_editor(&modal_entity, app_cx);
                            }
                        }),
                )
                .child(
                    Button::new("settings-rule-save")
                        .label(if modal.rule_saving {
                            "Saving..."
                        } else {
                            "Save rule"
                        })
                        .appearance(ButtonAppearance::Accent)
                        .size(ButtonSize::Compact)
                        .disabled(modal.rule_saving || modal.rules_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                start_settings_rule_save(&state_entity, &modal_entity, app_cx);
                            }
                        }),
                ),
        )
}

pub(crate) fn settings_rule_condition_row(
    row: SettingsRuleConditionRow,
    modal_entity: Entity<SettingsModal>,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let field_type = condition_field_type(&row.condition.field);
    let condition_id = row.condition.id.clone();
    let mut line = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().w(px(170.0)).child(row.field_dropdown.clone()));

    match field_type {
        ConditionFieldType::Text => {
            line = line
                .child(div().w(px(170.0)).child(row.operator_dropdown.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .child(row.value_input.clone()),
                );
        }
        ConditionFieldType::Boolean => {
            line = line
                .child(div().w(px(170.0)).child(row.bool_dropdown.clone()))
                .child(div().flex_1());
        }
        ConditionFieldType::Date => {
            line = line
                .child(
                    div()
                        .w(px(170.0))
                        .text_size(px(theme.typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child("YYYY-MM-DD"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .child(row.value_input.clone()),
                );
        }
    }

    line.child(
        Button::new(("settings-rule-remove-condition", row.key))
            .label("Remove")
            .appearance(ButtonAppearance::Subtle)
            .size(ButtonSize::Compact)
            .on_click({
                let modal_entity = modal_entity.clone();
                move |_, _, app_cx| {
                    modal_entity.update(app_cx, |modal, cx| {
                        modal.remove_rule_condition(&condition_id, cx);
                    });
                }
            }),
    )
}

pub(crate) fn settings_rule_action_row(
    row: SettingsRuleActionRow,
    modal_entity: Entity<SettingsModal>,
    can_remove: bool,
    _cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let key = row.key;
    let mut line = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().w(px(190.0)).child(row.action_dropdown.clone()));

    if rule_action_needs_folder(&row.action_type) {
        line = line.child(
            div()
                .flex_1()
                .min_w(px(220.0))
                .child(row.folder_dropdown.clone()),
        );
    } else {
        line = line.child(div().flex_1());
    }

    line.child(
        Button::new(("settings-rule-remove-action", key))
            .label("Remove")
            .appearance(ButtonAppearance::Subtle)
            .size(ButtonSize::Compact)
            .disabled(!can_remove)
            .on_click({
                let modal_entity = modal_entity.clone();
                move |_, _, app_cx| {
                    modal_entity.update(app_cx, |modal, cx| {
                        modal.remove_rule_action(key, cx);
                    });
                }
            }),
    )
}

pub(crate) fn settings_webhooks_body(
    modal_entity: &Entity<SettingsModal>,
    modal: &SettingsModal,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let state_entity = modal.state.clone();
    let status = modal.status.clone();

    let mut webhook_list = div().flex().flex_col().gap(px(6.0));
    if modal.webhooks_loading {
        webhook_list = webhook_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("Loading webhooks..."),
        );
    } else if modal.webhooks.is_empty() {
        webhook_list = webhook_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("No webhooks configured."),
        );
    } else {
        for webhook in &modal.webhooks {
            let webhook_id = webhook.id.clone();
            let busy = modal.webhook_busy_ids.contains(&webhook.id);
            webhook_list = webhook_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .child(
                        Checkbox::new(SharedString::from(format!(
                            "settings-webhook-enabled-{}",
                            webhook.id
                        )))
                        .checked(webhook.enabled)
                        .disabled(busy || modal.webhooks_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let webhook_id = webhook_id.clone();
                            move |new_state, _, _, app_cx| {
                                start_settings_webhook_toggle(
                                    &state_entity,
                                    &modal_entity,
                                    webhook_id.clone(),
                                    matches!(new_state, CheckboxState::Checked),
                                    app_cx,
                                );
                            }
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.body.size))
                                    .text_color(colors.on_neutral)
                                    .child(webhook.url.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(typography.caption.size))
                                    .text_color(colors.on_subtle)
                                    .child(format!(
                                        "Events: {}{}",
                                        webhook.events,
                                        if webhook.secret.is_some() {
                                            " - signed"
                                        } else {
                                            ""
                                        }
                                    )),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-webhook-delete-{}",
                            webhook.id
                        )))
                        .label("Delete")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(busy || modal.webhooks_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let webhook_id = webhook_id.clone();
                            move |_, _, app_cx| {
                                start_settings_webhook_delete(
                                    &state_entity,
                                    &modal_entity,
                                    webhook_id.clone(),
                                    app_cx,
                                );
                            }
                        }),
                    ),
            );
        }
    }

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(settings_section_title("Configured webhooks", cx))
        .child(webhook_list)
        .child(settings_section_title("Add webhook", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("URL", cx))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(220.0))
                        .child(modal.webhook_url_input.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Secret", cx))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(220.0))
                        .child(modal.webhook_secret_input.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Events", cx))
                .child(
                    Button::new("settings-webhook-events-new-message")
                        .label("New message")
                        .appearance(if modal.webhook_events == "new_message" {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                modal_entity.update(app_cx, |modal, cx| {
                                    modal.webhook_events = "new_message".to_string();
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    Button::new("settings-webhook-events-all")
                        .label("All")
                        .appearance(if modal.webhook_events == "all" {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                modal_entity.update(app_cx, |modal, cx| {
                                    modal.webhook_events = "all".to_string();
                                    cx.notify();
                                });
                            }
                        }),
                ),
        )
        .when_some(status, |body, (message, ok)| {
            body.child(
                div()
                    .text_size(px(typography.caption.size))
                    .text_color(if ok {
                        colors.status_success
                    } else {
                        colors.status_error
                    })
                    .child(message),
            )
        })
        .child(
            Button::new("settings-add-webhook")
                .label(if modal.adding_webhook {
                    "Adding..."
                } else {
                    "Add webhook"
                })
                .appearance(ButtonAppearance::Accent)
                .size(ButtonSize::Compact)
                .disabled(modal.adding_webhook || modal.webhooks_loading)
                .on_click({
                    let state_entity = state_entity.clone();
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        start_settings_webhook_create(&state_entity, &modal_entity, app_cx);
                    }
                }),
        )
}

pub(crate) fn settings_section_title(
    label: &'static str,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    div()
        .pt(px(4.0))
        .text_size(px(theme.typography.body.size))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.colors.on_neutral)
        .child(label)
}

pub(crate) fn settings_label(label: &'static str, cx: &mut Context<SettingsModal>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .w(px(120.0))
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

pub(crate) fn settings_readonly_row(
    label: &'static str,
    value: &str,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let body_size = theme.typography.body.size;
    let value_color = theme.colors.on_subtle;
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(settings_label(label, cx))
        .child(
            div()
                .text_size(px(body_size))
                .text_color(value_color)
                .child(value.to_string()),
        )
}
