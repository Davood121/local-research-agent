import type { AppEvent } from "./events";
import { useActivityStore } from "../stores/activityStore";
import { useChatStore } from "../stores/chatStore";
import { useMetricsStore } from "../stores/metricsStore";
import { useSystemStore } from "../stores/systemStore";

/** Single place that maps the wire union onto stores. */
export function routeAppEvent(event: AppEvent): void {
  switch (event.kind) {
    case "Telemetry":
      useSystemStore.getState().setTelemetry(event);
      break;
    case "Token":
      useActivityStore.getState().onToken(event);
      useChatStore.getState().onToken(event);
      break;
    case "Metrics":
      useMetricsStore.getState().onMetrics(event);
      break;
    case "Step":
      useActivityStore.getState().onStep(event);
      useChatStore.getState().onStep(event);
      break;
    case "Tool":
      useActivityStore.getState().onTool(event);
      break;
    case "Sources":
      useChatStore.getState().onSources(event);
      break;
    case "Error":
      useActivityStore.getState().onError(event);
      if (event.scope === "ollama" || event.scope === "research") {
        useChatStore.getState().onError(event);
      } else {
        useSystemStore.getState().setBridgeError(event.message);
      }
      break;
  }
}
