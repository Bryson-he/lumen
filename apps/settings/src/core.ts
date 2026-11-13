// The page's side of the connection to lumen-core (see src-tauri/src/core.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** An error from the core. `code` 0 means the core could not be reached. */
export class CoreError extends Error {
  constructor(
    public code: number,
    message: string,
  ) {
    super(message);
  }
}

/** Sends one JSON-RPC request to the core. */
export async function call<T>(method: string, params?: unknown): Promise<T> {
  try {
    return await invoke<T>("core_call", { method, params: params ?? null });
  } catch (error) {
    const e = error as { code?: number; message?: string };
    throw new CoreError(e?.code ?? 0, e?.message ?? String(error));
  }
}

/** Listens for events pushed by the core (themeChanged, gameModeChanged). */
export function onCoreEvent(handler: (method: string, params: unknown) => void): Promise<UnlistenFn> {
  return listen<{ method: string; params: unknown }>("core-event", (event) => handler(event.payload.method, event.payload.params));
}

/** Listens for the connection to the core going up or down. */
export function onConnection(handler: (connected: boolean, message: string | null) => void): Promise<UnlistenFn> {
  return listen<{ connected: boolean; message: string | null }>("core-connection", (event) =>
    handler(event.payload.connected, event.payload.message),
  );
}
