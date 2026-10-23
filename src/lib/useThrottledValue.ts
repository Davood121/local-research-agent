import { useEffect, useRef, useState } from "react";

/**
 * Coalesces fast store updates into at most one render per `intervalMs`.
 * The trailing update always wins, so the UI settles on the latest value.
 */
export function useThrottledValue<T>(value: T, intervalMs = 100): T {
  const [shown, setShown] = useState(value);
  const latest = useRef(value);
  const lastShownAt = useRef(0);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    latest.current = value;
    if (timer.current !== null) return;

    const wait = Math.max(0, intervalMs - (Date.now() - lastShownAt.current));
    timer.current = window.setTimeout(() => {
      timer.current = null;
      lastShownAt.current = Date.now();
      setShown(latest.current);
    }, wait);
  }, [value, intervalMs]);

  useEffect(
    () => () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
        // StrictMode runs this cleanup between the two development mounts.
        // Leaving the stale id behind would make the remounted effect think a
        // timer is still pending and never schedule another one.
        timer.current = null;
      }
    },
    [],
  );

  return shown;
}
