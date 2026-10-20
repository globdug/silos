import { useEffect, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { WebApp } from "../types";
import { api } from "../api";
import { SiteSearch } from "./SiteSearch";
import { openUrl } from "@tauri-apps/plugin-opener";
import { GearIcon, GitHubIcon, LockIcon, TrashIcon } from "./icons";
import { ConfirmTyped } from "./ConfirmTyped";
import logo from "../assets/logo.png";

interface Props {
  apps: WebApp[];
  onSelect: (appId: string, pin?: string | null) => Promise<void>;
  onCreate: (
    name: string,
    url: string,
    icon: string | null,
    fit: "cover" | "contain",
    rounded: boolean,
    background: string | null,
    paddingPercent: number,
  ) => Promise<void>;
  onDelete: (appId: string) => Promise<void>;
  onOpenGlobalSettings: () => void;
}

export function Dashboard({ apps, onSelect, onCreate, onDelete, onOpenGlobalSettings }: Props) {
  const { t } = useTranslation();
  const [iconCache, setIconCache] = useState<Record<string, string>>({});
  const [deleteTarget, setDeleteTarget] = useState<WebApp | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [openingId, setOpeningId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  function showToast(message: string) {
    if (toastTimer.current) clearTimeout(toastTimer.current);
    setToast(message);
    toastTimer.current = setTimeout(() => setToast(null), 2500);
  }

  function openApp(app: WebApp) {
    setOpeningId(app.id);
    return onSelect(app.id).finally(() => setOpeningId(null));
  }

  // Protected apps aren't PIN-checked here: open_app opens the window
  // regardless and, if locked, covers it with the in-window PIN overlay
  // (same one used for re-locked backgrounded apps and shortcut launches) —
  // a single PIN entry path instead of a separate dashboard modal.
  function handleCardClick(app: WebApp) {
    if (openingId) return;
    openApp(app);
  }

  function handleCardKeyDown(e: KeyboardEvent<HTMLDivElement>, app: WebApp) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleCardClick(app);
    }
  }

  async function handleConfirmDelete() {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await onDelete(deleteTarget.id);
      showToast(t("dashboard.appDeletedToast", { name: deleteTarget.name }));
      setDeleteTarget(null);
    } finally {
      setDeleting(false);
    }
  }

  useEffect(() => {
    apps.forEach((app) => {
      if (app.icon && !iconCache[app.id]) {
        api.readIconDataUrl(app.icon).then((dataUrl) => {
          setIconCache((prev) => ({ ...prev, [app.id]: dataUrl }));
        });
      }
    });
  }, [apps]);

  return (
    <div className="dashboard">
      <div className="dashboard-header">
        <div className="dashboard-title">
          <img src={logo} alt="" className="dashboard-logo" />
          <h1>Silos</h1>
        </div>
        <button className="ghost btn-icon-label" onClick={onOpenGlobalSettings} title={t("dashboard.settingsTitle")}>
          <GearIcon size={14} /> {t("dashboard.settings")}
        </button>
      </div>

      <div className="search-card">
        <SiteSearch
          searchPlaceholder={t("dashboard.searchPlaceholder")}
          confirmLabel={t("dashboard.createLabel")}
          showShape
          showBackground
          onConfirm={async ({ name, url, icon, fit, rounded, background, paddingPercent }) => {
            await onCreate(name, url, icon, fit, rounded, background, paddingPercent);
            showToast(t("dashboard.appAddedToast", { name }));
          }}
        />
      </div>

      {apps.length === 0 ? (
        <div className="empty-state">{t("dashboard.emptyState")}</div>
      ) : (
        <div className="app-grid">
          {apps.map((app) => (
            <div
              key={app.id}
              className={`app-card${openingId === app.id ? " app-card-opening" : ""}`}
              role="button"
              tabIndex={0}
              aria-label={`${app.name}${app.has_pin ? t("dashboard.protectedByPin") : ""}, ${t("dashboard.subspaceCount", { count: app.subspaces.length })}`}
              aria-busy={openingId === app.id}
              onClick={() => handleCardClick(app)}
              onKeyDown={(e) => handleCardKeyDown(e, app)}
            >
              <div className="app-card-actions">
                <button
                  className="app-card-settings ghost btn-icon-label"
                  onClick={(e) => {
                    e.stopPropagation();
                    api.openAppSettings(app.id);
                  }}
                  title={t("dashboard.appSettingsTitle")}
                >
                  <GearIcon size={14} />
                </button>
                <button
                  className="app-card-delete danger-ghost"
                  onClick={(e) => {
                    e.stopPropagation();
                    setDeleteTarget(app);
                  }}
                  title={t("dashboard.deleteAppTitle")}
                >
                  <TrashIcon size={14} />
                </button>
              </div>
              <div
                className={`app-card-icon ${!iconCache[app.id] ? "app-card-icon-fallback" : ""} ${
                  app.icon_style.rounded ? "" : "app-card-icon-square"
                }`}
                style={
                  {
                    background: app.icon_background_color ?? undefined,
                  } as CSSProperties
                }
              >
                {iconCache[app.id] ? (
                  <img
                    src={iconCache[app.id]}
                    alt=""
                    style={{ objectFit: app.icon_style.fit, padding: `${app.icon_style.padding_percent}%` }}
                  />
                ) : (
                  app.name.charAt(0).toUpperCase()
                )}
              </div>
              <div className="app-card-name">
                {app.name}
                {app.has_pin && (
                  <span className="app-card-pin-lock" title={t("dashboard.pinLockTitle")}>
                    <LockIcon />
                  </span>
                )}
              </div>
              <div className="app-card-url">{app.url}</div>
              <div className="app-card-meta">{t("dashboard.subspaceCount", { count: app.subspaces.length })}</div>
            </div>
          ))}
        </div>
      )}

      {deleteTarget && (
        <div className="modal-overlay" onClick={() => !deleting && setDeleteTarget(null)}>
          <div className="modal-box" onClick={(e) => e.stopPropagation()}>
            <h2>{t("dashboard.deleteConfirmTitle", { name: deleteTarget.name })}</h2>
            <p>{t("dashboard.deleteConfirmBody", { count: deleteTarget.subspaces.length })}</p>
            <ConfirmTyped
              label={deleting ? t("dashboard.deletingLabel") : t("dashboard.deleteLabel")}
              onConfirm={handleConfirmDelete}
              onCancel={() => setDeleteTarget(null)}
              busy={deleting}
            />
          </div>
        </div>
      )}

      {toast && (
        <div className="toast" role="status">
          {toast}
        </div>
      )}

      <div className="dashboard-footer">
        <span>
          {t("dashboard.footer", {
            version: __APP_VERSION__,
            date: new Date(__BUILD_DATE__).toLocaleDateString(),
            commit: __COMMIT_HASH__,
          })}
        </span>
        <a
          href="#"
          className="dashboard-footer-github"
          title="GitHub"
          onClick={(e) => {
            e.preventDefault();
            openUrl("https://github.com/globdug/silos");
          }}
        >
          <GitHubIcon size={13} />
        </a>
      </div>
    </div>
  );
}
