/**
 * Display names for the discovered AI providers, so a provider added in Settings shows the name it was given instead
 * of its internal `custom-ai-…` id (`GET /api/v1/ai/status`). Read only when `wanted` (there is a result to label);
 * if it cannot be read, the id is shown as it is — a label must never get in the way of the result itself.
 */
import { useCallback, useEffect, useState } from 'react';
import { aiqaApi } from '../services/aiqaApi';

export function useProviderNames(wanted: boolean): (id: string) => string {
  const [names, setNames] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!wanted) return;
    const controller = new AbortController();
    aiqaApi.ai
      .status(controller.signal)
      .then((s) => {
        if (controller.signal.aborted || !Array.isArray(s?.providers)) return;
        setNames(Object.fromEntries(s.providers.map((p) => [p.id, p.name])));
      })
      .catch(() => {
        // Keep showing ids.
      });
    return () => controller.abort();
  }, [wanted]);

  return useCallback((id: string) => names[id] ?? id, [names]);
}
