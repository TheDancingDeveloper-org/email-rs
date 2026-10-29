use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{AppError, Result};

// ── Domain types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub is_active: bool,
    pub match_mode: String, // "all" or "any"
    pub priority: i64,
    pub conditions: Vec<RuleCondition>,
    pub actions: Vec<RuleAction>,
}

#[derive(Debug, Clone)]
pub struct RuleCondition {
    pub id: String,
    pub field: String, // from, to, subject, body, has_attachment, is_read, is_flagged, date_after, date_before
    pub operator: String, // contains, not_contains, equals, starts_with
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct RuleAction {
    pub id: String,
    pub action_type: String, // mark_read, mark_unread, flag, unflag, archive, delete, move_to_folder
    pub action_value: Option<String>, // folder_id for move_to_folder
}

// ── DB row types ──────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct RuleRow {
    id: String,
    account_id: String,
    name: String,
    is_active: bool,
    match_mode: String,
    priority: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct RuleConditionRow {
    id: String,
    field: String,
    operator: String,
    value: String,
}

#[derive(Debug, sqlx::FromRow)]
struct RuleActionRow {
    id: String,
    action_type: String,
    action_value: Option<String>,
}

// ── RulesService ──────────────────────────────────────────────────────────────

pub struct RulesService {
    pool: SqlitePool,
}

impl RulesService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Fetch all rules for an account, each with their conditions and actions.
    pub async fn list_rules(&self, account_id: &str) -> Result<Vec<Rule>> {
        let rows = sqlx::query_as::<_, RuleRow>(
            "SELECT id, account_id, name, is_active, match_mode, priority
             FROM rules WHERE account_id = ? ORDER BY priority ASC, created_at ASC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;

        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            let conditions = self.load_conditions(&row.id).await?;
            let actions = self.load_actions(&row.id).await?;
            rules.push(Rule {
                id: row.id,
                account_id: row.account_id,
                name: row.name,
                is_active: row.is_active,
                match_mode: row.match_mode,
                priority: row.priority,
                conditions,
                actions,
            });
        }
        Ok(rules)
    }

    /// Fetch a single rule with its conditions and actions.
    pub async fn get_rule(&self, id: &str) -> Result<Rule> {
        let row = sqlx::query_as::<_, RuleRow>(
            "SELECT id, account_id, name, is_active, match_mode, priority FROM rules WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("rule {} not found", id)))?;

        let conditions = self.load_conditions(&row.id).await?;
        let actions = self.load_actions(&row.id).await?;

        Ok(Rule {
            id: row.id,
            account_id: row.account_id,
            name: row.name,
            is_active: row.is_active,
            match_mode: row.match_mode,
            priority: row.priority,
            conditions,
            actions,
        })
    }

    /// Create a new rule and return its id.
    pub async fn create_rule(
        &self,
        account_id: &str,
        name: &str,
        match_mode: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO rules (id, account_id, name, match_mode) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(account_id)
            .bind(name)
            .bind(match_mode)
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    /// Replace all conditions for a rule (delete then re-insert).
    pub async fn upsert_rule_conditions(
        &self,
        rule_id: &str,
        conditions: &[RuleCondition],
    ) -> Result<()> {
        sqlx::query("DELETE FROM rule_conditions WHERE rule_id = ?")
            .bind(rule_id)
            .execute(&self.pool)
            .await?;

        for (position, cond) in conditions.iter().enumerate() {
            let id = if cond.id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                cond.id.clone()
            };
            sqlx::query(
                "INSERT INTO rule_conditions (id, rule_id, field, operator, value, position)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(rule_id)
            .bind(&cond.field)
            .bind(&cond.operator)
            .bind(&cond.value)
            .bind(position as i64)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Replace all actions for a rule (delete then re-insert).
    pub async fn upsert_rule_actions(&self, rule_id: &str, actions: &[RuleAction]) -> Result<()> {
        sqlx::query("DELETE FROM rule_actions WHERE rule_id = ?")
            .bind(rule_id)
            .execute(&self.pool)
            .await?;

        for (position, action) in actions.iter().enumerate() {
            let id = if action.id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                action.id.clone()
            };
            sqlx::query(
                "INSERT INTO rule_actions (id, rule_id, action_type, action_value, position)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(rule_id)
            .bind(&action.action_type)
            .bind(&action.action_value)
            .bind(position as i64)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Delete a rule (cascades to conditions and actions).
    pub async fn delete_rule(&self, id: &str) -> Result<()> {
        let rows = sqlx::query("DELETE FROM rules WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if rows.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("rule {} not found", id)));
        }
        Ok(())
    }

    /// Toggle a rule's is_active flag.
    pub async fn toggle_rule(&self, id: &str) -> Result<()> {
        let rows = sqlx::query("UPDATE rules SET is_active = NOT is_active WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if rows.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("rule {} not found", id)));
        }
        Ok(())
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    async fn load_conditions(&self, rule_id: &str) -> Result<Vec<RuleCondition>> {
        let rows = sqlx::query_as::<_, RuleConditionRow>(
            "SELECT id, field, operator, value FROM rule_conditions WHERE rule_id = ? ORDER BY position ASC",
        )
        .bind(rule_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| RuleCondition {
                id: r.id,
                field: r.field,
                operator: r.operator,
                value: r.value,
            })
            .collect())
    }

    async fn load_actions(&self, rule_id: &str) -> Result<Vec<RuleAction>> {
        let rows = sqlx::query_as::<_, RuleActionRow>(
            "SELECT id, action_type, action_value FROM rule_actions WHERE rule_id = ? ORDER BY position ASC",
        )
        .bind(rule_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| RuleAction {
                id: r.id,
                action_type: r.action_type,
                action_value: r.action_value,
            })
            .collect())
    }
}

// ── Message evaluation ────────────────────────────────────────────────────────

pub struct MessageFields<'a> {
    pub subject: Option<&'a str>,
    pub from_name: Option<&'a str>,
    pub from_email: Option<&'a str>,
    pub to_json: Option<&'a str>,
    pub preview: Option<&'a str>,
    pub is_read: bool,
    pub is_flagged: bool,
    pub has_attachments: bool,
    pub date: Option<&'a str>,
}

/// Evaluate a rule against a message. Returns true if the rule matches.
pub fn evaluate_rule(rule: &Rule, msg: &MessageFields) -> bool {
    if rule.conditions.is_empty() {
        return false;
    }

    let results: Vec<bool> = rule
        .conditions
        .iter()
        .map(|cond| evaluate_condition(cond, msg))
        .collect();

    if rule.match_mode == "any" {
        results.iter().any(|&r| r)
    } else {
        // "all" (default)
        results.iter().all(|&r| r)
    }
}

fn evaluate_condition(cond: &RuleCondition, msg: &MessageFields) -> bool {
    match cond.field.as_str() {
        "from" => {
            let name_match = msg
                .from_name
                .map(|v| text_op(&cond.operator, v, &cond.value))
                .unwrap_or(false);
            let email_match = msg
                .from_email
                .map(|v| text_op(&cond.operator, v, &cond.value))
                .unwrap_or(false);
            name_match || email_match
        }
        "to" => msg
            .to_json
            .map(|v| text_op(&cond.operator, v, &cond.value))
            .unwrap_or(false),
        "subject" => msg
            .subject
            .map(|v| text_op(&cond.operator, v, &cond.value))
            .unwrap_or(false),
        "body" => msg
            .preview
            .map(|v| text_op(&cond.operator, v, &cond.value))
            .unwrap_or(false),
        "has_attachment" => {
            let expected = cond.value == "true";
            msg.has_attachments == expected
        }
        "is_read" => {
            let expected = cond.value == "true";
            msg.is_read == expected
        }
        "is_flagged" => {
            let expected = cond.value == "true";
            msg.is_flagged == expected
        }
        "date_after" => {
            if let Some(date) = msg.date {
                // date is RFC3339; compare prefix "YYYY-MM-DD" lexicographically
                let msg_date = &date[..date.len().min(10)];
                msg_date > cond.value.as_str()
            } else {
                false
            }
        }
        "date_before" => {
            if let Some(date) = msg.date {
                let msg_date = &date[..date.len().min(10)];
                msg_date < cond.value.as_str()
            } else {
                false
            }
        }
        _ => false,
    }
}

fn text_op(operator: &str, field_val: &str, cond_val: &str) -> bool {
    let field_lc = field_val.to_lowercase();
    let cond_lc = cond_val.to_lowercase();
    match operator {
        "contains" => field_lc.contains(&cond_lc),
        "not_contains" => !field_lc.contains(&cond_lc),
        "equals" => field_lc == cond_lc,
        "starts_with" => field_lc.starts_with(&cond_lc),
        _ => false,
    }
}

// ── Apply rules to a message ──────────────────────────────────────────────────

/// Load all active rules for the account and apply them to the given message.
/// Called during sync after each message upsert.
pub async fn apply_rules_to_message(
    pool: &SqlitePool,
    account_id: &str,
    msg_id: &str,
    fields: &MessageFields<'_>,
) -> Result<()> {
    let service = RulesService::new(pool.clone());
    let rules = service.list_rules(account_id).await?;

    for rule in &rules {
        if !rule.is_active {
            continue;
        }
        if !evaluate_rule(rule, fields) {
            continue;
        }

        // Rule matched — apply all its actions.
        for action in &rule.actions {
            apply_action(pool, account_id, msg_id, action).await;
        }
    }

    Ok(())
}

async fn apply_action(pool: &SqlitePool, account_id: &str, msg_id: &str, action: &RuleAction) {
    // BEGIN IMMEDIATE so a rule write waits out busy_timeout instead of
    // failing SQLITE_BUSY after a deferred read.
    let mut tx = match crate::db::begin_write(pool).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::warn!("rule action transaction failed: {}", error);
            return;
        }
    };

    let written: std::result::Result<bool, sqlx::Error> = async {
        match action.action_type.as_str() {
            "mark_read" => {
                sqlx::query("UPDATE messages SET is_read = 1 WHERE id = ?")
                    .bind(msg_id)
                    .execute(&mut *tx)
                    .await?;
            }
            "mark_unread" => {
                sqlx::query("UPDATE messages SET is_read = 0 WHERE id = ?")
                    .bind(msg_id)
                    .execute(&mut *tx)
                    .await?;
            }
            "flag" => {
                sqlx::query("UPDATE messages SET is_flagged = 1 WHERE id = ?")
                    .bind(msg_id)
                    .execute(&mut *tx)
                    .await?;
            }
            "unflag" => {
                sqlx::query("UPDATE messages SET is_flagged = 0 WHERE id = ?")
                    .bind(msg_id)
                    .execute(&mut *tx)
                    .await?;
            }
            "archive" => {
                let folder_id: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM folders WHERE account_id = ? AND special_use = 'archive' LIMIT 1",
                )
                .bind(account_id)
                .fetch_optional(&mut *tx)
                .await?;
                let Some(folder_id) = folder_id else {
                    return Ok(false);
                };
                sqlx::query("UPDATE messages SET folder_id = ? WHERE id = ?")
                    .bind(&folder_id)
                    .bind(msg_id)
                    .execute(&mut *tx)
                    .await?;
            }
            "delete" => {
                sqlx::query("DELETE FROM messages WHERE id = ?")
                    .bind(msg_id)
                    .execute(&mut *tx)
                    .await?;
            }
            "move_to_folder" => {
                let Some(folder_id) = &action.action_value else {
                    return Ok(false);
                };
                sqlx::query("UPDATE messages SET folder_id = ? WHERE id = ?")
                    .bind(folder_id)
                    .bind(msg_id)
                    .execute(&mut *tx)
                    .await?;
            }
            other => {
                tracing::warn!("unknown rule action type: {}", other);
                return Ok(false);
            }
        }
        Ok(true)
    }
    .await;

    match written {
        Ok(true) => {
            if let Err(error) = tx.commit().await {
                tracing::warn!(
                    "rule action '{}' commit failed for message {}: {}",
                    action.action_type,
                    msg_id,
                    error
                );
            }
        }
        Ok(false) => {}
        Err(error) => {
            tracing::warn!(
                "rule action '{}' failed for message {}: {}",
                action.action_type,
                msg_id,
                error
            );
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(match_mode: &str, conditions: Vec<RuleCondition>) -> Rule {
        Rule {
            id: "r1".into(),
            account_id: "acc1".into(),
            name: "Test".into(),
            is_active: true,
            match_mode: match_mode.to_string(),
            priority: 0,
            conditions,
            actions: vec![],
        }
    }

    fn cond(field: &str, operator: &str, value: &str) -> RuleCondition {
        RuleCondition {
            id: "c1".into(),
            field: field.into(),
            operator: operator.into(),
            value: value.into(),
        }
    }

    fn msg<'a>() -> MessageFields<'a> {
        MessageFields {
            subject: Some("Hello from Rust"),
            from_name: Some("Alice"),
            from_email: Some("alice@example.com"),
            to_json: Some(r#"["bob@example.com"]"#),
            preview: Some("This is a preview of the email body"),
            is_read: false,
            is_flagged: false,
            has_attachments: true,
            date: Some("2024-03-15T10:00:00Z"),
        }
    }

    #[test]
    fn text_contains_subject() {
        let rule = make_rule("all", vec![cond("subject", "contains", "Rust")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn text_not_contains_subject() {
        let rule = make_rule("all", vec![cond("subject", "not_contains", "Python")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn text_equals_case_insensitive() {
        let rule = make_rule("all", vec![cond("subject", "equals", "hello from rust")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn text_starts_with() {
        let rule = make_rule("all", vec![cond("subject", "starts_with", "Hello")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn from_matches_name() {
        let rule = make_rule("all", vec![cond("from", "contains", "alice")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn from_matches_email() {
        let rule = make_rule("all", vec![cond("from", "contains", "example.com")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn boolean_has_attachment() {
        let rule = make_rule("all", vec![cond("has_attachment", "equals", "true")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn boolean_is_read_false() {
        let rule = make_rule("all", vec![cond("is_read", "equals", "false")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn date_after() {
        let rule = make_rule("all", vec![cond("date_after", "equals", "2024-01-01")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn date_before() {
        let rule = make_rule("all", vec![cond("date_before", "equals", "2025-01-01")]);
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn match_any_one_matches() {
        let rule = make_rule(
            "any",
            vec![
                cond("subject", "contains", "Python"),
                cond("from", "contains", "alice"),
            ],
        );
        assert!(evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn match_all_one_fails() {
        let rule = make_rule(
            "all",
            vec![
                cond("subject", "contains", "Rust"),
                cond("from", "contains", "bob"),
            ],
        );
        assert!(!evaluate_rule(&rule, &msg()));
    }

    #[test]
    fn empty_conditions_returns_false() {
        let rule = make_rule("all", vec![]);
        assert!(!evaluate_rule(&rule, &msg()));
    }
}
