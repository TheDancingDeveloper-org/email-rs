use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::rules::{RuleAction, RuleCondition, RulesService};
use crate::state::AppState;

// ── Wire-types (serialized to/from the API) ────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleConditionDto {
    pub id: String,
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleActionDto {
    pub id: String,
    pub action_type: String,
    pub action_value: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub is_active: bool,
    pub match_mode: String,
    pub priority: i64,
    pub conditions: Vec<RuleConditionDto>,
    pub actions: Vec<RuleActionDto>,
}

impl From<crate::rules::Rule> for RuleDto {
    fn from(r: crate::rules::Rule) -> Self {
        RuleDto {
            id: r.id,
            account_id: r.account_id,
            name: r.name,
            is_active: r.is_active,
            match_mode: r.match_mode,
            priority: r.priority,
            conditions: r
                .conditions
                .into_iter()
                .map(|c| RuleConditionDto {
                    id: c.id,
                    field: c.field,
                    operator: c.operator,
                    value: c.value,
                })
                .collect(),
            actions: r
                .actions
                .into_iter()
                .map(|a| RuleActionDto {
                    id: a.id,
                    action_type: a.action_type,
                    action_value: a.action_value,
                })
                .collect(),
        }
    }
}

// ── Request bodies ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListRulesQuery {
    #[serde(alias = "accountId")]
    pub account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionInput {
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionInput {
    pub action_type: String,
    pub action_value: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRuleBody {
    pub account_id: String,
    pub name: String,
    pub match_mode: Option<String>,
    pub conditions: Option<Vec<ConditionInput>>,
    pub actions: Option<Vec<ActionInput>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRuleBody {
    pub name: Option<String>,
    pub match_mode: Option<String>,
    pub conditions: Option<Vec<ConditionInput>>,
    pub actions: Option<Vec<ActionInput>>,
}

// ── Handlers ──────────────────────────────────────────────────────────────

pub async fn list_rules(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListRulesQuery>,
) -> Result<Json<Vec<RuleDto>>> {
    let svc = RulesService::new(state.pool.clone());
    let rules = svc.list_rules(&q.account_id).await?;
    Ok(Json(rules.into_iter().map(RuleDto::from).collect()))
}

pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateRuleBody>,
) -> Result<Json<RuleDto>> {
    let svc = RulesService::new(state.pool.clone());
    let match_mode = body.match_mode.unwrap_or_else(|| "all".to_string());
    let id = svc
        .create_rule(&body.account_id, &body.name, &match_mode)
        .await?;

    if let Some(conditions) = body.conditions {
        let conds: Vec<RuleCondition> = conditions
            .into_iter()
            .map(|c| RuleCondition {
                id: String::new(),
                field: c.field,
                operator: c.operator,
                value: c.value,
            })
            .collect();
        svc.upsert_rule_conditions(&id, &conds).await?;
    }

    if let Some(actions) = body.actions {
        let acts: Vec<RuleAction> = actions
            .into_iter()
            .map(|a| RuleAction {
                id: String::new(),
                action_type: a.action_type,
                action_value: a.action_value,
            })
            .collect();
        svc.upsert_rule_actions(&id, &acts).await?;
    }

    let rule = svc.get_rule(&id).await?;
    Ok(Json(RuleDto::from(rule)))
}

pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateRuleBody>,
) -> Result<Json<RuleDto>> {
    let svc = RulesService::new(state.pool.clone());

    // Verify exists first.
    svc.get_rule(&id).await?;

    if let Some(ref name) = body.name {
        sqlx::query("UPDATE rules SET name = ? WHERE id = ?")
            .bind(name)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(ref match_mode) = body.match_mode {
        sqlx::query("UPDATE rules SET match_mode = ? WHERE id = ?")
            .bind(match_mode)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(conditions) = body.conditions {
        let conds: Vec<RuleCondition> = conditions
            .into_iter()
            .map(|c| RuleCondition {
                id: String::new(),
                field: c.field,
                operator: c.operator,
                value: c.value,
            })
            .collect();
        svc.upsert_rule_conditions(&id, &conds).await?;
    }

    if let Some(actions) = body.actions {
        let acts: Vec<RuleAction> = actions
            .into_iter()
            .map(|a| RuleAction {
                id: String::new(),
                action_type: a.action_type,
                action_value: a.action_value,
            })
            .collect();
        svc.upsert_rule_actions(&id, &acts).await?;
    }

    let rule = svc.get_rule(&id).await?;
    Ok(Json(RuleDto::from(rule)))
}

pub async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let svc = RulesService::new(state.pool.clone());
    svc.delete_rule(&id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn toggle_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RuleDto>> {
    let svc = RulesService::new(state.pool.clone());
    svc.toggle_rule(&id).await?;
    let rule = svc.get_rule(&id).await?;
    Ok(Json(RuleDto::from(rule)))
}

#[cfg(test)]
mod tests {
    use super::ListRulesQuery;

    #[test]
    fn list_rules_query_accepts_frontend_snake_case() {
        let q: ListRulesQuery =
            serde_json::from_value(serde_json::json!({ "account_id": "acc1" })).unwrap();
        assert_eq!(q.account_id, "acc1");
    }

    #[test]
    fn list_rules_query_accepts_camel_case_alias() {
        let q: ListRulesQuery =
            serde_json::from_value(serde_json::json!({ "accountId": "acc1" })).unwrap();
        assert_eq!(q.account_id, "acc1");
    }
}
