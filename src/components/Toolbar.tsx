import { FormEvent, MouseEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import type { TabInfo } from "../types";
import { api } from "../api";
import { DownloadIcon } from "./icons";

interface Props {
  appId: string;
  subspaceId: string;
}

export function Toolbar({ appId, subspaceId }: Props) {
  const { t } = useTranslation();
  const [tabs, setTabs] = useState<TabInfo[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [urlInput, setUrlInput] = useState("");
  const [editingUrl, setEditingUrl] = useState(false);
  const [homeUrl, setHomeUrl] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [unseenDownloads, setUnseenDownloads] = useState(0);
  const [flashKey, setFlashKey] = useState(0);
  const seenDownloadIds = useRef<Set<string>>(new Set());
  const latestDownloadIds = useRef<string[]>([]);

  function openDownloads() {
    seenDownloadIds.current = new Set(latestDownloadIds.current);
    setUnseenDownloads(0);
    api.openDownloadsPopup(appId);
  }

  async function refresh() {
    const [nextTabs, active] = await Promise.all([
      api.getTabs(appId, subspaceId),
      api.getActiveTab(appId, subspaceId),
    ]);
    setTabs(nextTabs);
    setActiveId(active);
  }

  useEffect(() => {
    refresh();
    const unlisten = listen("tabs-changed", refresh);
    return () => {
      unlisten.then((f) => f());
    };
  }, [appId, subspaceId]);

  useEffect(() => {
    // Items already listed when this toolbar loads count as seen; anything
    // that appears afterwards flashes the button and adds to the badge until
    // the popup is opened.
    let known: Set<string> | null = null;
    const refreshDownloads = () =>
      api.listDownloads(appId).then((items) => {
        setDownloading(items.some((i) => i.status === "in_progress"));
        if (known === null) {
          known = new Set(items.map((i) => i.id));
          seenDownloadIds.current = new Set(known);
          latestDownloadIds.current = items.map((i) => i.id);
          return;
        }
        const fresh = items.filter((i) => !known!.has(i.id));
        fresh.forEach((i) => known!.add(i.id));
        if (fresh.length > 0) setFlashKey((k) => k + 1);
        setUnseenDownloads(items.filter((i) => !seenDownloadIds.current.has(i.id)).length);
        latestDownloadIds.current = items.map((i) => i.id);
      });
    refreshDownloads();
    const unlisten = listen<string>("downloads-changed", (e) => {
      if (e.payload === appId) refreshDownloads();
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [appId]);

  useEffect(() => {
    api.listApps().then((apps) => {
      const app = apps.find((a) => a.id === appId);
      const subspace = app?.subspaces.find((s) => s.id === subspaceId);
      setHomeUrl(subspace?.start_url ?? app?.url ?? null);
    });
  }, [appId, subspaceId]);

  const activeTab = tabs.find((t) => t.id === activeId) ?? null;

  useEffect(() => {
    if (activeTab && !editingUrl) {
      setUrlInput(activeTab.url);
    }
  }, [activeTab?.url, editingUrl]);

  async function handleNewTab() {
    await api.openTab(appId, subspaceId);
  }

  async function handleCloseTab(e: MouseEvent, tabId: string) {
    e.stopPropagation();
    await api.closeTab(appId, subspaceId, tabId);
  }

  async function handleUrlSubmit(e: FormEvent) {
    e.preventDefault();
    if (!activeId || !urlInput.trim()) return;
    const target = /^[a-z]+:\/\//i.test(urlInput.trim()) ? urlInput.trim() : `https://${urlInput.trim()}`;
    setEditingUrl(false);
    await api.navigateTab(appId, subspaceId, activeId, target);
  }

  return (
    <div className="toolbar">
      <div className="toolbar-tabs">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            className={`toolbar-tab ${tab.id === activeId ? "toolbar-tab-active" : ""}`}
            onClick={() => api.switchTab(appId, subspaceId, tab.id)}
            title={tab.title}
          >
            <span className="toolbar-tab-title">{tab.title || t("toolbar.newTabFallbackTitle")}</span>
            {tabs.length > 1 && (
              <button className="toolbar-tab-close" onClick={(e) => handleCloseTab(e, tab.id)}>
                ×
              </button>
            )}
          </div>
        ))}
        <button className="toolbar-tab-add" onClick={handleNewTab} title={t("toolbar.newTabTitle")}>
          +
        </button>
      </div>

      <div className="toolbar-nav">
        <button
          className="ghost toolbar-nav-btn"
          onClick={() => activeId && api.tabBack(appId, subspaceId, activeId)}
          title={t("toolbar.backTitle")}
        >
          ‹
        </button>
        <button
          className="ghost toolbar-nav-btn"
          onClick={() => activeId && api.tabForward(appId, subspaceId, activeId)}
          title={t("toolbar.forwardTitle")}
        >
          ›
        </button>
        <button
          className="ghost toolbar-nav-btn"
          onClick={() => activeId && api.reloadTab(appId, subspaceId, activeId)}
          title={t("toolbar.reloadTitle")}
        >
          ↻
        </button>
        <button
          className="ghost toolbar-nav-btn"
          disabled={!homeUrl}
          onClick={() => activeId && homeUrl && api.navigateTab(appId, subspaceId, activeId, homeUrl)}
          title={t("toolbar.homeTitle")}
        >
          ⌂
        </button>
        <button
          key={flashKey}
          className={`ghost toolbar-nav-btn toolbar-downloads-btn ${downloading ? "toolbar-downloads-active" : ""} ${flashKey > 0 ? "toolbar-downloads-flash" : ""}`}
          onClick={openDownloads}
          title={t("toolbar.downloadsTitle")}
        >
          <DownloadIcon size={15} />
          {unseenDownloads > 0 && <span className="toolbar-downloads-badge">{unseenDownloads}</span>}
        </button>
        <form className="toolbar-url-form" onSubmit={handleUrlSubmit}>
          <input
            value={urlInput}
            onChange={(e) => {
              setEditingUrl(true);
              setUrlInput(e.target.value);
            }}
            onFocus={() => setEditingUrl(true)}
            onBlur={() => setEditingUrl(false)}
            placeholder={t("toolbar.urlPlaceholder")}
          />
        </form>
      </div>
    </div>
  );
}
