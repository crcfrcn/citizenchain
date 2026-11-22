// 统一加载并向页面提供CID类型、机构和行政区元信息。

import { useCallback, useEffect, useState } from 'react';
import type { AdminAuth } from '../authentication/types';
import type { CidMetaResult } from './api';
import { loadCachedCidMeta } from './cache';

export interface UseCidMetaResult {
  meta: CidMetaResult | null;
  loading: boolean;
  error: string | null;
  reload: () => Promise<void>;
}

export function useCidMeta(auth: AdminAuth | null): UseCidMetaResult {
  const [meta, setMeta] = useState<CidMetaResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    if (!auth) {
      setMeta(null);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const next = await loadCachedCidMeta(auth);
      setMeta(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [auth]);

  useEffect(() => {
    let cancelled = false;
    if (!auth) {
      setMeta(null);
      setError(null);
      return () => {
        cancelled = true;
      };
    }
    setLoading(true);
    setError(null);
    loadCachedCidMeta(auth)
      .then((next) => {
        if (!cancelled) setMeta(next);
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [auth]);

  return { meta, loading, error, reload };
}
