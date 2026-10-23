/** Turn Tauri's serializable command error into a readable string. */
export function describeError(error: unknown): string {
  if (error instanceof Error) {
    if (
      error.message.includes("__TAURI_INTERNALS__") ||
      error.message.includes("reading 'invoke'")
    ) {
      return "The desktop backend is unavailable in a browser preview. Run `npm run tauri:dev`.";
    }
    return error.message;
  }
  if (typeof error === "string") return error;
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return "The local backend did not return a usable response.";
}
