import { KeyboardEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../api";
import { SiteSearch } from "./SiteSearch";
import type { ConfirmResult } from "./SiteSearch";
import type { SessionGroupInfo, WebApp } from "../types";

/// The tab a "new subspace from this tab" popup was opened from.
export interface FromTab {
  url: string;
  name: string;
  subspaceId: string;
  tabId: string;
}

interface Props {
  appId: string;
  from?: FromTab;
}

const ISOLATED = "__isolated__";

// Reading the tab's favicon is quick, but it ends in a download with no
// timeout of its own; past this the popup opens without it rather than
// hanging blank.
const TAB_ICON_TIMEOUT_MS = 4000;

export function AddSubspacePopup({ appId, from }: Props) {
  const { t } = useTranslation();
  const [app, setApp] = useState<WebApp | null>(null);
  const [groups, setGroups] = useState<SessionGroupInfo[]>([]);
  const [sessionChoice, setSessionChoice] = useState(ISOLATED);
  // undefined while the tab's icon is still being read, null if it has none.
  const [tabIcon, setTabIcon] = useState<string | null | undefined>(from ? undefined : null);

  useEffect(() => {
    api.listApps().then((apps) => {
      const found = apps.find((a) => a.id === appId) ?? null;
      setApp(found);
      // Opened from a tab: join the session of the subspace it was open in.
      const group = from && found?.subspaces.find((s) => s.id === from.subspaceId)?.session_group;
      if (group) setSessionChoice(group);
    });
    api.listSessionGroups(appId).then(setGroups);
  }, [appId]);

  useEffect(() => {
    if (!from) return;
    let cancelled = false;
    Promise.race([
      api.getTabIcon(appId, from.subspaceId, from.tabId),
      new Promise<never>((_, reject) => setTimeout(reject, TAB_ICON_TIMEOUT_MS)),
    ])
      .then((path) => !cancelled && setTabIcon(path))
      .catch(() => !cancelled && setTabIcon(null));
    return () => {
      cancelled = true;
    };
  }, [appId, from?.subspaceId, from?.tabId]);

  async function handleConfirm({ name, url, icon, background }: ConfirmResult) {
    if (!app) return;
    const startUrl = url.trim() && url.trim() !== app.url ? url.trim() : null;
    const sessionGroup = sessionChoice === ISOLATED ? null : sessionChoice;
    await api.createSubspace(appId, name, startUrl, icon, sessionGroup, background);
    await getCurrentWindow().close();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") getCurrentWindow().close();
  }

  if (!app || tabIcon === undefined) return null;

  return (
    <div className="add-popup" onKeyDown={handleKeyDown}>
      <SiteSearch
        initial={{
          name: from?.name.trim() || app.name,
          url: from?.url ?? app.url,
          icon: tabIcon ?? app.icon,
        }}
        searchPlaceholder={t("addSubspacePopup.searchPlaceholder")}
        confirmLabel={t("addSubspacePopup.createLabel")}
        showBackground
        defaultBackground={app.icon_background_color}
        // The tab is still open, so its own favicon beats re-fetching the
        // site logged out (which gives Google's generic logo for any Google
        // service). Only while the URL is still the tab's.
        fetchFavicon={(url) =>
          from && url.trim() === from.url ? api.getTabIcon(appId, from.subspaceId, from.tabId) : api.fetchFaviconForUrl(url)
        }
        onConfirm={handleConfirm}
        onCancel={() => getCurrentWindow().close()}
        extraFields={
          <label className="settings-field">
            <span>{t("addSubspacePopup.sessionLabel")}</span>
            <select value={sessionChoice} onChange={(e) => setSessionChoice(e.target.value)}>
              <option value={ISOLATED}>{t("addSubspacePopup.isolatedSession")}</option>
              {groups.map((g) => (
                <option key={g.group} value={g.group}>
                  {t("addSubspacePopup.sharedSessionWith", { label: g.label })}
                </option>
              ))}
            </select>
          </label>
        }
      />
    </div>
  );
}
