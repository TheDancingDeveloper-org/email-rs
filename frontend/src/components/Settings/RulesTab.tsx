import { useState, useEffect, useCallback } from 'react';
import { api } from '../../api/client';
import styles from './RulesTab.module.css';
import { ConditionBuilder, defaultConditionGroup } from '../ConditionBuilder/ConditionBuilder';
import { useAppStore } from '../../store';
import type { ConditionGroup, Folder } from '../../types';

// ── Types ─────────────────────────────────────────────────────────────────────

interface RuleConditionDto {
  id: string;
  field: string;
  operator: string;
  value: string;
}

interface RuleActionDto {
  id: string;
  actionType: string;
  actionValue: string | null;
}

interface RuleDto {
  id: string;
  accountId: string;
  name: string;
  isActive: boolean;
  matchMode: string;
  priority: number;
  conditions: RuleConditionDto[];
  actions: RuleActionDto[];
}

interface ActionInput {
  actionType: string;
  actionValue: string | null;
}

// ── Action type definitions ───────────────────────────────────────────────────

const ACTION_TYPES: { value: string; label: string; needsFolder: boolean }[] = [
  { value: 'mark_read',       label: 'Mark as read',      needsFolder: false },
  { value: 'mark_unread',     label: 'Mark as unread',    needsFolder: false },
  { value: 'flag',            label: 'Flag message',      needsFolder: false },
  { value: 'unflag',          label: 'Unflag message',    needsFolder: false },
  { value: 'archive',         label: 'Archive',           needsFolder: false },
  { value: 'delete',          label: 'Delete',            needsFolder: false },
  { value: 'move_to_folder',  label: 'Move to folder',    needsFolder: true  },
];

// ── RuleEditor ─────────────────────────────────────────────────────────────

interface RuleEditorProps {
  accountId: string;
  folders: Folder[];
  initial: RuleDto | null;
  onSaved: () => void;
  onCancel: () => void;
}

function RuleEditor({ accountId, folders, initial, onSaved, onCancel }: RuleEditorProps) {
  const [name, setName] = useState(initial?.name ?? '');
  const [conditionGroup, setConditionGroup] = useState<ConditionGroup>(
    initial
      ? {
          match: (initial.matchMode as 'all' | 'any') ?? 'all',
          conditions: initial.conditions.map((c) => ({
            id: c.id,
            field: c.field as import('../../types').ConditionField,
            operator: c.operator as import('../../types').ConditionOperator,
            value: c.value,
          })),
        }
      : defaultConditionGroup(),
  );
  const [actions, setActions] = useState<ActionInput[]>(
    initial?.actions.map((a) => ({ actionType: a.actionType, actionValue: a.actionValue })) ??
      [{ actionType: 'mark_read', actionValue: null }],
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const addAction = useCallback(() => {
    setActions((prev) => [...prev, { actionType: 'mark_read', actionValue: null }]);
  }, []);

  const removeAction = useCallback((index: number) => {
    setActions((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const updateAction = useCallback((index: number, patch: Partial<ActionInput>) => {
    setActions((prev) =>
      prev.map((a, i) => (i === index ? { ...a, ...patch } : a)),
    );
  }, []);

  const handleSave = useCallback(async () => {
    if (!name.trim()) {
      setError('Rule name is required');
      return;
    }
    setSaving(true);
    setError(null);

    const body = {
      accountId,
      name: name.trim(),
      matchMode: conditionGroup.match,
      conditions: conditionGroup.conditions.map((c) => ({
        field: c.field,
        operator: c.operator,
        value: c.value,
      })),
      actions: actions.map((a) => ({
        actionType: a.actionType,
        actionValue: a.actionValue || null,
      })),
    };

    try {
      let resp: Response;
      if (initial) {
        resp = await api.put(`/api/v1/rules/${initial.id}`, body);
      } else {
        resp = await api.post('/api/v1/rules', body);
      }
      if (resp.ok) {
        onSaved();
      } else {
        const data = await resp.json().catch(() => ({}));
        setError((data as { error?: string }).error ?? 'Save failed');
      }
    } catch {
      setError('Network error');
    } finally {
      setSaving(false);
    }
  }, [accountId, name, conditionGroup, actions, initial, onSaved]);

  return (
    <div className={styles.editor}>
      <div className={styles.editorTitle}>{initial ? 'Edit rule' : 'New rule'}</div>

      <div className={styles.fieldRow}>
        <span className={styles.label}>Name</span>
        <input
          className={styles.input}
          type="text"
          placeholder="Rule name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </div>

      <div className={styles.sectionLabel}>Conditions</div>
      <ConditionBuilder value={conditionGroup} onChange={setConditionGroup} />

      <div className={styles.sectionLabel} style={{ marginTop: 14 }}>Actions</div>
      <div className={styles.actionList}>
        {actions.map((action, i) => {
          const typeDef = ACTION_TYPES.find((t) => t.value === action.actionType);
          return (
            <div key={i} className={styles.actionRow}>
              <select
                className={styles.actionSelect}
                value={action.actionType}
                onChange={(e) =>
                  updateAction(i, { actionType: e.target.value, actionValue: null })
                }
              >
                {ACTION_TYPES.map((t) => (
                  <option key={t.value} value={t.value}>{t.label}</option>
                ))}
              </select>
              {typeDef?.needsFolder && (
                <select
                  className={styles.actionSelect}
                  value={action.actionValue ?? ''}
                  onChange={(e) => updateAction(i, { actionValue: e.target.value || null })}
                >
                  <option value="">Select folder…</option>
                  {folders.map((f) => (
                    <option key={f.id} value={f.id}>{f.name}</option>
                  ))}
                </select>
              )}
              <button
                type="button"
                className={styles.removeBtn}
                onClick={() => removeAction(i)}
                aria-label="Remove action"
              >
                ✕
              </button>
            </div>
          );
        })}
      </div>
      <button type="button" className={styles.addBtn} onClick={addAction}>
        + Add action
      </button>

      {error && <div className={styles.error}>{error}</div>}

      <div className={styles.editorFooter}>
        <button type="button" className={styles.cancelBtn} onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          className={styles.saveBtn}
          onClick={handleSave}
          disabled={saving}
        >
          {saving ? 'Saving…' : 'Save rule'}
        </button>
      </div>
    </div>
  );
}

// ── RulesTab ──────────────────────────────────────────────────────────────────

interface RulesTabProps {
  accountId: string;
}

export function RulesTab({ accountId }: RulesTabProps) {
  const { folders } = useAppStore();
  const accountFolders = folders.filter((f) => f.accountId === accountId);

  const [rules, setRules] = useState<RuleDto[]>([]);
  const [editingRule, setEditingRule] = useState<RuleDto | null | 'new'>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    api.get<RuleDto[]>(`/api/v1/rules?account_id=${encodeURIComponent(accountId)}`)
      .then((data) => {
        setRules(data);
        setError(null);
      })
      .catch((e) => setError(e instanceof Error ? `Failed to load rules: ${e.message}` : 'Failed to load rules'));
  }, [accountId]);

  useEffect(() => { load(); }, [load]);

  const handleToggle = useCallback(async (rule: RuleDto) => {
    await api.post(`/api/v1/rules/${rule.id}/toggle`);
    load();
  }, [load]);

  const handleDelete = useCallback(async (id: string) => {
    await api.delete(`/api/v1/rules/${id}`);
    load();
  }, [load]);

  const handleSaved = useCallback(() => {
    setEditingRule(null);
    load();
  }, [load]);

  const handleCancel = useCallback(() => {
    setEditingRule(null);
  }, []);

  return (
    <div>
      {editingRule !== null && (
        <RuleEditor
          accountId={accountId}
          folders={accountFolders}
          initial={editingRule === 'new' ? null : editingRule}
          onSaved={handleSaved}
          onCancel={handleCancel}
        />
      )}

      {editingRule === null && (
        <>
          {error && <div className={styles.error}>{error}</div>}

          {!error && rules.length === 0 ? (
            <div className={styles.empty}>No rules configured for this account.</div>
          ) : (
            <div className={styles.ruleList}>
              {rules.map((rule) => (
                <div
                  key={rule.id}
                  className={`${styles.ruleItem}${!rule.isActive ? ` ${styles.ruleInactive}` : ''}`}
                >
                  <input
                    type="checkbox"
                    className={styles.toggle}
                    checked={rule.isActive}
                    onChange={() => handleToggle(rule)}
                    title={rule.isActive ? 'Disable rule' : 'Enable rule'}
                  />
                  <span className={styles.ruleName}>{rule.name}</span>
                  <span className={styles.ruleMeta}>
                    {rule.conditions.length} condition{rule.conditions.length !== 1 ? 's' : ''},&nbsp;
                    {rule.actions.length} action{rule.actions.length !== 1 ? 's' : ''}
                  </span>
                  <button
                    type="button"
                    className={styles.ruleBtn}
                    onClick={() => setEditingRule(rule)}
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    className={styles.dangerBtn}
                    onClick={() => handleDelete(rule.id)}
                  >
                    Delete
                  </button>
                </div>
              ))}
            </div>
          )}

          <button
            type="button"
            className={styles.newRuleBtn}
            onClick={() => setEditingRule('new')}
          >
            + New rule
          </button>
        </>
      )}
    </div>
  );
}
