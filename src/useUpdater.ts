import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { UpdateInfo } from "./types";

export type UpdaterStatus = "idle" | "checking" | "upToDate" | "installing" | "error";

/// Outcome of a check: `error` is set when the check itself failed, which is
/// different from "no newer version" (`update` null, `error` null).
export type CheckResult = { update: UpdateInfo | null; error: string | null };

/// Checks GitHub for a newer Silos and installs it. `check()` reports its
/// own failures through `status`/`error` and its return value; the automatic
/// callers just ignore them (a failed background check shouldn't bother
/// anyone).
export function useUpdater() {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [status, setStatus] = useState<UpdaterStatus>("idle");
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<[number, number]>("update-progress", (e) => {
      const [done, total] = e.payload;
      setProgress(total > 0 ? Math.round((done / total) * 100) : null);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function check(): Promise<CheckResult> {
    setStatus("checking");
    setError(null);
    try {
      const found = await api.checkForUpdate();
      setUpdate(found);
      setStatus(found ? "idle" : "upToDate");
      return { update: found, error: null };
    } catch (e) {
      const message = String(e);
      setError(message);
      setStatus("error");
      return { update: null, error: message };
    }
  }

  async function install() {
    setStatus("installing");
    setError(null);
    setProgress(null);
    try {
      // On success the app restarts, so this normally never returns.
      await api.installUpdate();
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }

  return { update, status, progress, error, check, install };
}

export type Updater = ReturnType<typeof useUpdater>;
