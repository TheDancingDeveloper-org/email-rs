import { useState, useEffect, useRef, useCallback } from 'react';
import { api, reportApiError } from '../../api/client';
import styles from './RecipientInput.module.css';
import type { Suggestion } from '../../types';

interface Props {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
}

export function RecipientInput({ value, onChange, placeholder, autoFocus }: Props) {
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [activeIdx, setActiveIdx] = useState(0);
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // The current token being typed is the part after the last comma.
  function currentToken(v: string) {
    const parts = v.split(',');
    return parts[parts.length - 1].trim();
  }

  function replaceToken(v: string, replacement: string): string {
    const parts = v.split(',');
    parts[parts.length - 1] = ` ${replacement}`;
    return parts.join(',').replace(/^,\s*/, '');
  }

  useEffect(() => {
    const tok = currentToken(value);
    if (tok.length < 2) {
      setSuggestions([]);
      setOpen(false);
      return;
    }
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      try {
        const data = await api.get<{ messages: Suggestion[] }>(`/api/v1/search/suggest?q=${encodeURIComponent(tok)}`);
        // Dedupe by fromEmail.
        const seen = new Set<string>();
        const unique = (data.messages ?? []).filter((s) => {
          if (!s.fromEmail || seen.has(s.fromEmail)) return false;
          seen.add(s.fromEmail);
          return true;
        });
        setSuggestions(unique.slice(0, 6));
        setOpen(unique.length > 0);
        setActiveIdx(0);
      } catch (e) { reportApiError(e); }
    }, 150);
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current); };
  }, [value]);

  const select = useCallback((s: Suggestion) => {
    const addr = s.fromName
      ? `${s.fromName} <${s.fromEmail}>`
      : (s.fromEmail ?? '');
    onChange(replaceToken(value, addr) + ', ');
    setSuggestions([]);
    setOpen(false);
  }, [value, onChange]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!open || suggestions.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, suggestions.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter' || e.key === 'Tab') {
      const s = suggestions[activeIdx];
      if (s) { e.preventDefault(); select(s); }
    } else if (e.key === 'Escape') {
      setOpen(false);
    }
  }, [open, suggestions, activeIdx, select]);

  return (
    <div className={styles.wrapper} ref={wrapperRef}>
      <input
        className={styles.input}
        type="text"
        value={value}
        onChange={(e) => { onChange(e.target.value); }}
        onKeyDown={handleKeyDown}
        onFocus={() => { if (suggestions.length > 0) setOpen(true); }}
        placeholder={placeholder}
        autoFocus={autoFocus}
        autoComplete="off"
      />
      {open && suggestions.length > 0 && (
        <div className={styles.dropdown}>
          {suggestions.map((s, i) => (
            <div
              key={s.id}
              className={`${styles.item} ${i === activeIdx ? styles.active : ''}`}
              onMouseEnter={() => setActiveIdx(i)}
              onMouseDown={(e) => { e.preventDefault(); select(s); }}
            >
              {s.fromName && <span className={styles.name}>{s.fromName}</span>}
              <span className={styles.email}>{s.fromEmail}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
