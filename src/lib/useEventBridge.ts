import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { describeError } from "./errors";
import { listenAppEvents } from "./events";
import { routeAppEvent } from "./route";
import { useSystemStore } from "../stores/systemStore";

/**
 * Installs the one-way Rust -> UI event bridge.
 *
 * `subscribe_events` is idempotent on the Rust side, which keeps React 18
 * StrictMode's double mount from installing two forwarders.
 */
export function useEventBridge(): void {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    const start = async () => {
      try {
        await invoke("subscribe_events");
        const stop = await listenAppEvents(routeAppEvent);
        if (disposed) {
          stop();
        } else {
          unlisten = stop;
          useSystemStore.getState().setBridgeError(null);
        }
      } catch (error) {
        useSystemStore.getState().setBridgeError(describeError(error));
      }
    };

    void start();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
}
