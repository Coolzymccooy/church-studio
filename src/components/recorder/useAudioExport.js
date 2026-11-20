import { useCallback, useEffect, useState } from 'react';

import { describeExportError, normalizeExportProgress } from '../../lib/audioExport.js';

/**
 * useAudioExport(controller) — one MP3/M4A export at a time with its
 * progress. `run(format, action)` resolves to the ExportResult or null;
 * failures land in `error` instead of throwing. Pass null to stay idle
 * (browser build).
 */
export function useAudioExport(controller) {
  const [job, setJob] = useState(null);
  const [fraction, setFraction] = useState(0);
  const [result, setResult] = useState(null);
  const [error, setError] = useState(null);

  useEffect(() => {
    if (!controller) return undefined;
    const sub = controller.subscribeProgress((raw) => setFraction(normalizeExportProgress(raw).fraction));
    return () => sub.dispose();
  }, [controller]);

  const run = useCallback(async (format, action) => {
    setJob({ format });
    setFraction(0);
    setError(null);
    setResult(null);
    try {
      const next = await action();
      setResult(next ?? null);
      return next ?? null;
    } catch (err) {
      setError(describeExportError(err));
      return null;
    } finally {
      setJob(null);
    }
  }, []);

  return {
    job,
    fraction,
    result,
    error,
    run,
    dismissError: () => setError(null),
    clearResult: () => setResult(null),
  };
}
