use crate::prelude::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct AdvancedConditionRow {
    pub(crate) key: u64,
    pub(crate) condition: Condition,
    pub(crate) value_input: Entity<TextInput>,
    pub(crate) field_dropdown: Entity<Dropdown>,
    pub(crate) operator_dropdown: Entity<Dropdown>,
    pub(crate) bool_dropdown: Entity<Dropdown>,
}

#[derive(Clone)]
pub(crate) struct AdvancedSearchModal {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) focused_on_open: bool,
    pub(crate) match_mode: MatchMode,
    pub(crate) rows: Vec<AdvancedConditionRow>,
    pub(crate) next_condition_id: u64,
}

impl AdvancedSearchModal {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let mut modal = Self {
            state,
            focus_handle: cx.focus_handle(),
            focused_on_open: false,
            match_mode: MatchMode::All,
            rows: Vec::new(),
            next_condition_id: 1,
        };
        modal.reset_to_group(None, cx);
        modal
    }

    pub(crate) fn reset_to_group(&mut self, group: Option<ConditionGroup>, cx: &mut Context<Self>) {
        self.rows.clear();
        self.next_condition_id = 1;
        let group = group.unwrap_or_else(default_condition_group);
        self.match_mode = group.match_mode;
        for condition in group.conditions {
            self.add_condition(condition, cx);
        }
        if self.rows.is_empty() {
            let id = self.take_condition_id();
            self.add_condition(new_advanced_condition(id), cx);
        }
        cx.notify();
    }

    pub(crate) fn take_condition_id(&mut self) -> String {
        let id = format!("native-condition-{}", self.next_condition_id);
        self.next_condition_id = self.next_condition_id.saturating_add(1);
        id
    }

    pub(crate) fn add_condition(&mut self, condition: Condition, cx: &mut Context<Self>) {
        let key = self.next_condition_id;
        self.next_condition_id = self.next_condition_id.saturating_add(1);
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
                            modal.set_condition_value(&condition_id, value.to_string());
                            cx.notify();
                        });
                    }
                })
        });
        let field_condition_id = condition.id.clone();
        let field_modal = cx.entity();
        let field_selected = condition_field_value(&condition.field);
        let field_dropdown = cx.new(move |_| {
            condition_field_dropdown(
                ("advanced-field", key),
                field_selected,
                field_modal.clone(),
                field_condition_id.clone(),
            )
        });
        let operator_condition_id = condition.id.clone();
        let operator_modal = cx.entity();
        let operator_selected = condition_operator_value(&condition.operator);
        let operator_dropdown = cx.new(move |_| {
            condition_operator_dropdown(
                ("advanced-op", key),
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
            boolean_dropdown(
                ("advanced-bool", key),
                bool_selected,
                bool_modal.clone(),
                bool_condition_id.clone(),
            )
        });
        self.rows.push(AdvancedConditionRow {
            key,
            condition,
            value_input,
            field_dropdown,
            operator_dropdown,
            bool_dropdown,
        });
    }

    pub(crate) fn add_blank_condition(&mut self, cx: &mut Context<Self>) {
        let id = self.take_condition_id();
        self.add_condition(new_advanced_condition(id), cx);
        cx.notify();
    }

    pub(crate) fn remove_condition(&mut self, condition_id: &str, cx: &mut Context<Self>) {
        self.rows
            .retain(|row| row.condition.id.as_str() != condition_id);
        if self.rows.is_empty() {
            let id = self.take_condition_id();
            self.add_condition(new_advanced_condition(id), cx);
        }
        cx.notify();
    }

    pub(crate) fn set_condition_field(
        &mut self,
        condition_id: &str,
        field: ConditionField,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.field = field.clone();
            row.condition.operator = ConditionOperator::Contains;
            row.condition.value = default_condition_value(&field).to_string();
            let value = row.condition.value.clone();
            row.value_input.update(cx, |input, cx| {
                input.set_value(value, cx);
            });
            row.operator_dropdown.update(cx, |dropdown, cx| {
                dropdown.set_selected(condition_operator_value(&ConditionOperator::Contains), cx);
            });
            row.bool_dropdown.update(cx, |dropdown, cx| {
                dropdown.set_selected(default_condition_value(&field), cx);
            });
            cx.notify();
        }
    }

    pub(crate) fn set_condition_operator(
        &mut self,
        condition_id: &str,
        operator: ConditionOperator,
    ) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.operator = operator;
        }
    }

    pub(crate) fn set_condition_value(&mut self, condition_id: &str, value: String) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.value = value;
        }
    }

    pub(crate) fn set_match_mode(&mut self, match_mode: MatchMode) {
        self.match_mode = match_mode;
    }

    pub(crate) fn condition_group(&self) -> ConditionGroup {
        ConditionGroup {
            match_mode: self.match_mode.clone(),
            conditions: self.rows.iter().map(|row| row.condition.clone()).collect(),
        }
    }

    pub(crate) fn has_active_condition(&self) -> bool {
        condition_group_has_active(&self.condition_group())
    }
}

impl Render for AdvancedSearchModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        if !state.modals.advanced_search_open {
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
        backdrop.a = 0.58;
        let has_active = self.has_active_condition();
        let modal_entity = cx.entity();

        let mut conditions = div().flex().flex_col().gap(px(8.0));
        for row in self.rows.clone() {
            conditions = conditions.child(advanced_condition_row(row, modal_entity.clone(), cx));
        }

        div()
            .absolute()
            .inset_0()
            .size_full()
            .tab_index(0)
            .key_context("AdvancedSearchModal")
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .bg(backdrop)
            .on_key_down({
                let state_entity = self.state.clone();
                let modal_entity = modal_entity.clone();
                move |ev: &KeyDownEvent, _, app_cx| {
                    if handle_advanced_search_modal_key(
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
                    close_advanced_search_modal(&state_entity, app_cx);
                }
            })
            .child(
                div()
                    .w(px(760.0))
                    .max_h(px(560.0))
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
                            .child(
                                div()
                                    .text_size(px(typography.subtitle.size))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.on_neutral)
                                    .child("Advanced search"),
                            )
                            .child(
                                Button::new("advanced-search-close")
                                    .label("Close")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        move |_, _, app_cx| {
                                            close_advanced_search_modal(&state_entity, app_cx);
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .id("advanced-search-scroll")
                            .overflow_y_scroll()
                            .px(px(18.0))
                            .py(px(16.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(advanced_label("Match", cx))
                                    .child(
                                        Button::new("advanced-match-all")
                                            .label("All")
                                            .appearance(if self.match_mode == MatchMode::All {
                                                ButtonAppearance::Neutral
                                            } else {
                                                ButtonAppearance::Subtle
                                            })
                                            .size(ButtonSize::Compact)
                                            .on_click({
                                                let modal_entity = modal_entity.clone();
                                                move |_, _, app_cx| {
                                                    modal_entity.update(app_cx, |modal, cx| {
                                                        modal.set_match_mode(MatchMode::All);
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    )
                                    .child(
                                        Button::new("advanced-match-any")
                                            .label("Any")
                                            .appearance(if self.match_mode == MatchMode::Any {
                                                ButtonAppearance::Neutral
                                            } else {
                                                ButtonAppearance::Subtle
                                            })
                                            .size(ButtonSize::Compact)
                                            .on_click({
                                                let modal_entity = modal_entity.clone();
                                                move |_, _, app_cx| {
                                                    modal_entity.update(app_cx, |modal, cx| {
                                                        modal.set_match_mode(MatchMode::Any);
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(typography.body.size))
                                            .text_color(colors.on_subtle)
                                            .child("of the following conditions"),
                                    ),
                            )
                            .child(conditions)
                            .child(
                                Button::new("advanced-add-condition")
                                    .label("Add condition")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let modal_entity = modal_entity.clone();
                                        move |_, _, app_cx| {
                                            modal_entity.update(app_cx, |modal, cx| {
                                                modal.add_blank_condition(cx);
                                            });
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(colors.stroke_neutral_subtle)
                            .px(px(18.0))
                            .py(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Button::new("advanced-clear")
                                    .label("Clear")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let modal_entity = modal_entity.clone();
                                        move |_, _, app_cx| {
                                            modal_entity.update(app_cx, |modal, cx| {
                                                modal.reset_to_group(None, cx);
                                            });
                                        }
                                    }),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("advanced-cancel")
                                    .label("Cancel")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        move |_, _, app_cx| {
                                            close_advanced_search_modal(&state_entity, app_cx);
                                        }
                                    }),
                            )
                            .child(
                                Button::new("advanced-submit")
                                    .label("Search")
                                    .appearance(ButtonAppearance::Accent)
                                    .size(ButtonSize::Compact)
                                    .disabled(!has_active)
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        let modal_entity = modal_entity.clone();
                                        move |_, _, app_cx| {
                                            let group = modal_entity
                                                .update(app_cx, |modal, _| modal.condition_group());
                                            start_advanced_search(&state_entity, group, app_cx);
                                        }
                                    }),
                            ),
                    ),
            )
    }
}

pub(crate) fn advanced_condition_row(
    row: AdvancedConditionRow,
    modal_entity: Entity<AdvancedSearchModal>,
    cx: &mut Context<AdvancedSearchModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let field_type = condition_field_type(&row.condition.field);
    let condition_id = row.condition.id.clone();
    let mut line = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().w(px(160.0)).child(row.field_dropdown.clone()));

    match field_type {
        ConditionFieldType::Text => {
            line = line
                .child(div().w(px(156.0)).child(row.operator_dropdown.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .child(row.value_input.clone()),
                );
        }
        ConditionFieldType::Boolean => {
            line = line
                .child(div().w(px(156.0)).child(row.bool_dropdown.clone()))
                .child(div().flex_1());
        }
        ConditionFieldType::Date => {
            line = line
                .child(
                    div()
                        .w(px(156.0))
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
        Button::new(("advanced-remove", row.key))
            .label("Remove")
            .appearance(ButtonAppearance::Subtle)
            .size(ButtonSize::Compact)
            .on_click({
                let modal_entity = modal_entity.clone();
                move |_, _, app_cx| {
                    modal_entity.update(app_cx, |modal, cx| {
                        modal.remove_condition(&condition_id, cx);
                    });
                }
            }),
    )
}

pub(crate) fn condition_field_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<AdvancedSearchModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("from", "From")
        .option("to", "To")
        .option("cc", "Cc")
        .option("subject", "Subject")
        .option("body", "Body")
        .option("filename", "Filename")
        .option("has_attachment", "Has attachment")
        .option("is_read", "Is read")
        .option("is_flagged", "Is starred")
        .option("date_after", "Date after")
        .option("date_before", "Date before")
        .on_select(move |value, app_cx| {
            let Some(field) = parse_condition_field(value.as_ref()) else {
                return;
            };
            modal_entity.update(app_cx, |modal, cx| {
                modal.set_condition_field(&condition_id, field, cx);
            });
        })
}

pub(crate) fn condition_operator_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<AdvancedSearchModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("contains", "contains")
        .option("not_contains", "doesn't contain")
        .option("equals", "equals")
        .option("starts_with", "starts with")
        .on_select(move |value, app_cx| {
            let Some(operator) = parse_condition_operator(value.as_ref()) else {
                return;
            };
            modal_entity.update(app_cx, |modal, cx| {
                modal.set_condition_operator(&condition_id, operator);
                cx.notify();
            });
        })
}

pub(crate) fn boolean_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<AdvancedSearchModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("true", "Yes")
        .option("false", "No")
        .on_select(move |value, app_cx| {
            modal_entity.update(app_cx, |modal, cx| {
                modal.set_condition_value(&condition_id, value.to_string());
                cx.notify();
            });
        })
}

pub(crate) fn rule_condition_field_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<SettingsModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("from", "From")
        .option("to", "To")
        .option("cc", "Cc")
        .option("subject", "Subject")
        .option("body", "Body")
        .option("filename", "Filename")
        .option("has_attachment", "Has attachment")
        .option("is_read", "Is read")
        .option("is_flagged", "Is starred")
        .option("date_after", "Date after")
        .option("date_before", "Date before")
        .on_select(move |value, app_cx| {
            let Some(field) = parse_condition_field(value.as_ref()) else {
                return;
            };
            modal_entity.update(app_cx, |modal, cx| {
                modal.set_rule_condition_field(&condition_id, field, cx);
            });
        })
}

pub(crate) fn rule_condition_operator_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<SettingsModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("contains", "contains")
        .option("not_contains", "doesn't contain")
        .option("equals", "equals")
        .option("starts_with", "starts with")
        .on_select(move |value, app_cx| {
            let Some(operator) = parse_condition_operator(value.as_ref()) else {
                return;
            };
            modal_entity.update(app_cx, |modal, cx| {
                modal.set_rule_condition_operator(&condition_id, operator);
                cx.notify();
            });
        })
}

pub(crate) fn rule_boolean_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<SettingsModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("true", "Yes")
        .option("false", "No")
        .on_select(move |value, app_cx| {
            modal_entity.update(app_cx, |modal, cx| {
                modal.set_rule_condition_value(&condition_id, value.to_string());
                cx.notify();
            });
        })
}

pub(crate) fn rule_action_type_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &str,
    modal_entity: Entity<SettingsModal>,
    action_key: u64,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected.to_string())
        .option("mark_read", "Mark as read")
        .option("mark_unread", "Mark as unread")
        .option("flag", "Flag message")
        .option("unflag", "Unflag message")
        .option("archive", "Archive")
        .option("delete", "Delete")
        .option("move_to_folder", "Move to folder")
        .on_select(move |value, app_cx| {
            let action_type = value.to_string();
            if !is_supported_rule_action(&action_type) {
                return;
            }
            modal_entity.update(app_cx, |modal, cx| {
                modal.set_rule_action_type(action_key, action_type);
                cx.notify();
            });
        })
}

pub(crate) fn rule_action_folder_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: Option<String>,
    folders: Vec<Folder>,
    modal_entity: Entity<SettingsModal>,
    action_key: u64,
) -> Dropdown {
    let mut dropdown = Dropdown::new(id).placeholder("Select folder");
    if let Some(selected) = selected.filter(|value| !value.trim().is_empty()) {
        dropdown = dropdown.selected(selected);
    }
    for folder in folders {
        dropdown = dropdown.option(folder.id.clone(), rule_folder_option_label(&folder));
    }
    dropdown.on_select(move |value, app_cx| {
        let selected = value.to_string();
        modal_entity.update(app_cx, |modal, cx| {
            modal.set_rule_action_value(action_key, Some(selected));
            cx.notify();
        });
    })
}

pub(crate) fn advanced_label(
    label: &'static str,
    cx: &mut Context<AdvancedSearchModal>,
) -> gpui::Div {
    let theme = cx.theme();
    div()
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConditionFieldType {
    Text,
    Boolean,
    Date,
}

pub(crate) fn condition_field_type(field: &ConditionField) -> ConditionFieldType {
    match field {
        ConditionField::HasAttachment | ConditionField::IsRead | ConditionField::IsFlagged => {
            ConditionFieldType::Boolean
        }
        ConditionField::DateAfter | ConditionField::DateBefore => ConditionFieldType::Date,
        _ => ConditionFieldType::Text,
    }
}

pub(crate) fn default_condition_group() -> ConditionGroup {
    ConditionGroup {
        match_mode: MatchMode::All,
        conditions: vec![new_advanced_condition("native-condition-1".to_string())],
    }
}

pub(crate) fn new_advanced_condition(id: String) -> Condition {
    Condition {
        id,
        field: ConditionField::From,
        operator: ConditionOperator::Contains,
        value: String::new(),
    }
}

pub(crate) fn default_condition_value(field: &ConditionField) -> &'static str {
    if condition_field_type(field) == ConditionFieldType::Boolean {
        "true"
    } else {
        ""
    }
}

pub(crate) fn condition_group_has_active(group: &ConditionGroup) -> bool {
    group
        .conditions
        .iter()
        .any(|condition| !condition.value.trim().is_empty())
}

pub(crate) fn condition_field_value(field: &ConditionField) -> &'static str {
    match field {
        ConditionField::From => "from",
        ConditionField::To => "to",
        ConditionField::Cc => "cc",
        ConditionField::Subject => "subject",
        ConditionField::Body => "body",
        ConditionField::Filename => "filename",
        ConditionField::HasAttachment => "has_attachment",
        ConditionField::IsRead => "is_read",
        ConditionField::IsFlagged => "is_flagged",
        ConditionField::DateAfter => "date_after",
        ConditionField::DateBefore => "date_before",
    }
}

pub(crate) fn parse_condition_field(value: &str) -> Option<ConditionField> {
    match value {
        "from" => Some(ConditionField::From),
        "to" => Some(ConditionField::To),
        "cc" => Some(ConditionField::Cc),
        "subject" => Some(ConditionField::Subject),
        "body" => Some(ConditionField::Body),
        "filename" => Some(ConditionField::Filename),
        "has_attachment" => Some(ConditionField::HasAttachment),
        "is_read" => Some(ConditionField::IsRead),
        "is_flagged" => Some(ConditionField::IsFlagged),
        "date_after" => Some(ConditionField::DateAfter),
        "date_before" => Some(ConditionField::DateBefore),
        _ => None,
    }
}

pub(crate) fn condition_operator_value(operator: &ConditionOperator) -> &'static str {
    match operator {
        ConditionOperator::Contains => "contains",
        ConditionOperator::NotContains => "not_contains",
        ConditionOperator::Equals => "equals",
        ConditionOperator::StartsWith => "starts_with",
    }
}

pub(crate) fn parse_condition_operator(value: &str) -> Option<ConditionOperator> {
    match value {
        "contains" => Some(ConditionOperator::Contains),
        "not_contains" => Some(ConditionOperator::NotContains),
        "equals" => Some(ConditionOperator::Equals),
        "starts_with" => Some(ConditionOperator::StartsWith),
        _ => None,
    }
}
