import { useEffect, useState } from "react";
import type { KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DownloadItem } from "../types";
import { api } from "../api";

interface Props {
  appId: string;
}

export function DownloadsPopup({ appId }: Props) {
  const { t } = useTranslation();
  const [items, setItems] = useState<DownloadItem[]>([]);

  async function refresh() {
    setItems(await api.listDownloads(appId));
  }

  useEffect(() => {
    refresh();
    const unlistenChanged = listen<string>("downloads-changed", (e) => {
      if (e.payload === appId) refresh();
    });
    // Like a menu: dismiss as soon as focus moves elsewhere.
    const unlistenFocus = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused) getCurrentWindow().close();
    });
    return () => {
      unlistenChanged.then((f) => f());
      unlistenFocus.then((f) => f());
    };
  }, [appId]);

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") getCurrentWindow().close();
  }

  const hasFinished = items.some((i) => i.status !== "in_progress");

  return (
    <div className="downloads-popup" onKeyDown={handleKeyDown} tabIndex={-1}>
      <div className="downloads-popup-header">
        <h3>{t("downloads.title")}</h3>
        <button className="ghost" disabled={!hasFinished} onClick={() => api.clearDownloads(appId)}>
          {t("downloads.clear")}
        </button>
      </div>

      {items.length === 0 ? (
        <p className="downloads-empty">{t("downloads.empty")}</p>
      ) : (
        <ul className="downloads-list">
          {items.map((item) => (
            <li key={item.id} className="downloads-row">
              <div className="downloads-row-name" title={item.file_name}>
                {item.file_name}
              </div>
              <div className={`downloads-row-status downloads-status-${item.status}`}>
                {item.status === "in_progress"
                  ? t("downloads.inProgress")
                  : item.status === "completed"
                    ? t("downloads.completed")
                    : t("downloads.failed")}
              </div>
              <div className="downloads-row-path" title={item.path}>
                {item.path}
              </div>
              {item.status === "completed" && (
                <div className="downloads-row-actions">
                  <button className="ghost" onClick={() => api.openDownload(appId, item.id)}>
                    {t("downloads.open")}
                  </button>
                  <button className="ghost" onClick={() => api.revealDownload(appId, item.id)}>
                    {t("downloads.reveal")}
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
