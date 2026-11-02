import { api } from '../api/client';
import { useCallback, useEffect, useRef, useState } from 'react';

export interface SyncProgress {
  syncing: boolean;
  done: number;
  total: number;
  folder: string;
}

export function useSyncEvents(onSyncComplete: () => void): SyncProgress | null {
  const [progress, setProgress] = useState<SyncProgress | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const es = new EventSource('/api/v1/events');
    es.addEventListener('sync', (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data as string) as {
          type: string;
          accountId?: string;
          folderCount?: number;
          done?: number;
          total?: number;
          folder?: string;
        };
        if (data.type === 'sync_start') {
          if (timerRef.current) clearTimeout(timerRef.current);
          setProgress({ syncing: true, done: 0, total: data.folderCount ?? 0, folder: '' });
        } else if (data.type === 'sync_folder_done') {
          setProgress({ syncing: true, done: data.done ?? 0, total: data.total ?? 0, folder: data.folder ?? '' });
        } else if (data.type === 'sync_complete') {
          setProgress(p => p ? { ...p, syncing: false } : null);
          onSyncComplete();
          timerRef.current = setTimeout(() => setProgress(null), 3000);
        }
      } catch {
        onSyncComplete();
      }
    });
    es.onerror = () => es.close();
    return () => {
      es.close();
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [onSyncComplete]);

  return progress;
}

interface UseApiOptions<T> {
  immediate?: boolean;
  initialData?: T;
}

interface UseApiResult<T> {
  data: T | undefined;
  loading: boolean;
  error: string | null;
  refetch: () => void;
}

export function useApi<T>(
  url: string,
  options: UseApiOptions<T> = {},
): UseApiResult<T> {
  const { immediate = true, initialData } = options;
  const [data, setData] = useState<T | undefined>(initialData);
  const [loading, setLoading] = useState(immediate);
  const [error, setError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const fetch_ = useCallback(() => {
    if (abortRef.current) abortRef.current.abort();
    const ctrl = new AbortController();
    abortRef.current = ctrl;

    setLoading(true);
    setError(null);

    api.get<T>(url, { signal: ctrl.signal })
      .then((json) => {
        setData(json);
        setLoading(false);
      })
      .catch((err: unknown) => {
        if (err instanceof Error && err.name === 'AbortError') return;
        setError(err instanceof Error ? err.message : String(err));
        setLoading(false);
      });
  }, [url]);

  useEffect(() => {
    if (immediate) fetch_();
    return () => abortRef.current?.abort();
  }, [fetch_, immediate]);

  return { data, loading, error, refetch: fetch_ };
}

// POST helper: errors are thrown to the caller, who shows them, so the global report is skipped.
export function apiPost<T>(url: string, body: unknown): Promise<T> {
  return api.post<T>(url, body, { silent: true });
}
