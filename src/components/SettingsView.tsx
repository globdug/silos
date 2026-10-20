import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Subspace, WebApp } from "../types";
import { api } from "../api";
import { IconPicker } from "./IconPicker";
import { DEFAULT_ICON_BG } from "../colors";
import { ConfirmTyped } from "./ConfirmTyped";

interface Props {
  appId: string;
  subspaceId: string;
}

type Section = "info" | "session" | "icon";

type ToastFn = (message: string, type?: "success" | "error") => void;

export function SettingsView({ appId, subspaceId }: Props) {
  const { t } = useTranslation();
  const [app, setApp] = useState<WebApp | null>(null);
  const [section, setSection] = useState<Section>("info");
  const [iconUrl, setIconUrl] = useState<string | undefined>();
  const [toast, setToast] = useState<{ message: string; type: "success" | "error" } | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  function showToast(message: string, type: "success" | "error" = "success") {
    if (toastTimer.current) clearTimeout(toastTimer.current);
    setToast({ message, type });
    toastTimer.current = setTimeout(() => setToast(null), 2500);
  }

  async function refresh() {
    const apps = await api.listApps();
    const found = apps.find((a) => a.id === appId) ?? null;
    setApp(found);
    return found;
  }

  useEffect(() => {
    refresh();
  }, [appId, subspaceId]);

  const subspace = app?.subspaces.find((s) => s.id === subspaceId) ?? null;

  const effectiveIcon = subspace?.icon ?? app?.icon ?? undefined;

  useEffect(() => {
    if (effectiveIcon) {
      api.readIconDataUrl(effectiveIcon).then(setIconUrl);
    } else {
      setIconUrl(undefined);
    }
  }, [effectiveIcon]);

  if (!app || !subspace) return null;

  return (
    <div className="settings-window">
      <nav className="settings-nav">
        <button className={section === "info" ? "active" : ""} onClick={() => setSection("info")}>
          {t("settingsView.nav.info")}
        </button>
        <button className={section === "session" ? "active" : ""} onClick={() => setSection("session")}>
          {t("settingsView.nav.session")}
        </button>
        <button className={section === "icon" ? "active" : ""} onClick={() => setSection("icon")}>
          {t("settingsView.nav.icon")}
        </button>
      </nav>

      <div className="settings-panel">
        {section === "info" && (
          <InfoSection app={app} subspace={subspace} onChanged={refresh} onToast={showToast} />
        )}
        {section === "session" && (
          <SessionSection app={app} subspace={subspace} onChanged={refresh} onToast={showToast} />
        )}
        {section === "icon" && (
          <IconSection
            app={app}
            subspace={subspace}
            iconUrl={iconUrl}
            iconPath={effectiveIcon}
            onChanged={refresh}
            onToast={showToast}
          />
        )}
      </div>

      {toast && (
        <div className={`toast${toast.type === "error" ? " toast-error" : ""}`} role="status">
          {toast.message}
        </div>
      )}
    </div>
  );
}

function SaveActions({ onSave, onCancel }: { onSave: () => void; onCancel: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="settings-actions">
      <button onClick={onSave}>{t("common.save")}</button>
      <button className="ghost" onClick={onCancel}>
        {t("common.cancel")}
      </button>
    </div>
  );
}

function InfoSection({
  app,
  subspace,
  onChanged,
  onToast,
}: {
  app: WebApp;
  subspace: Subspace;
  onChanged: () => void;
  onToast: ToastFn;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(subspace.name);
  const [startUrl, setStartUrl] = useState(subspace.start_url ?? "");

  useEffect(() => {
    setName(subspace.name);
    setStartUrl(subspace.start_url ?? "");
  }, [subspace.id]);

  const isDirty = name !== subspace.name || startUrl !== (subspace.start_url ?? "");

  function cancel() {
    setName(subspace.name);
    setStartUrl(subspace.start_url ?? "");
  }

  async function save() {
    try {
      if (name.trim() && name.trim() !== subspace.name) {
        await api.renameSubspace(app.id, subspace.id, name.trim());
      }
      if (startUrl !== (subspace.start_url ?? "")) {
        await api.setSubspaceStartUrl(app.id, subspace.id, startUrl.trim() || null);
      }
      onChanged();
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  return (
    <div className="settings-section">
      <h2>{t("settingsView.info.title")}</h2>

      <label className="settings-field">
        <span>{t("settingsView.info.titleLabel")}</span>
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </label>

      <label className="settings-field">
        <span>{t("settingsView.info.startUrlLabel")}</span>
        <input value={startUrl} onChange={(e) => setStartUrl(e.target.value)} placeholder={app.url} />
        <small>{t("settingsView.info.startUrlHint", { url: app.url })}</small>
      </label>

      {isDirty && <SaveActions onSave={save} onCancel={cancel} />}
    </div>
  );
}

function SessionSection({
  app,
  subspace,
  onChanged,
  onToast,
}: {
  app: WebApp;
  subspace: Subspace;
  onChanged: () => void;
  onToast: ToastFn;
}) {
  const { t } = useTranslation();
  const [confirmingClear, setConfirmingClear] = useState(false);
  const [confirmingRemove, setConfirmingRemove] = useState(false);

  // "" = own session; otherwise the session group to share.
  const savedSharing = subspace.session_group === subspace.id ? "" : subspace.session_group;
  const [sharing, setSharing] = useState(savedSharing);

  useEffect(() => {
    setSharing(savedSharing);
  }, [subspace.id, savedSharing]);

  const isDirty = sharing !== savedSharing;

  async function save() {
    try {
      await api.setSubspaceSessionGroup(app.id, subspace.id, sharing || subspace.id);
      onChanged();
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  async function clearData() {
    await api.clearSubspaceData(app.id, subspace.id);
    setConfirmingClear(false);
  }

  async function removeSubspace() {
    await api.deleteSubspace(app.id, subspace.id);
    await getCurrentWindow().close();
  }

  return (
    <div className="settings-section">
      <h2>{t("settingsView.session.title")}</h2>

      <label className="settings-field">
        <span>{t("settingsView.session.sharingLabel")}</span>
        <select value={sharing} onChange={(e) => setSharing(e.target.value)}>
          <option value="">{t("settingsView.session.ownSession")}</option>
          {app.subspaces
            .filter((o) => o.id !== subspace.id)
            .map((o) => (
              <option key={o.id} value={o.session_group}>
                {t("settingsView.session.shareWith", { name: o.name })}
              </option>
            ))}
        </select>
        <small>{t("settingsView.session.sharingHint")}</small>
      </label>

      {isDirty && <SaveActions onSave={save} onCancel={() => setSharing(savedSharing)} />}

      <div className="settings-danger-zone">
        <h3>{t("settingsView.session.dangerZone")}</h3>

        {confirmingClear ? (
          <ConfirmTyped label={t("settingsView.session.confirmClear")} onConfirm={clearData} onCancel={() => setConfirmingClear(false)} />
        ) : (
          <button className="ghost" onClick={() => setConfirmingClear(true)}>
            {t("settingsView.session.clearDataButton")}
          </button>
        )}

        {confirmingRemove ? (
          <ConfirmTyped
            label={t("settingsView.session.confirmDelete")}
            onConfirm={removeSubspace}
            onCancel={() => setConfirmingRemove(false)}
          />
        ) : (
          <button className="danger-ghost" onClick={() => setConfirmingRemove(true)}>
            {t("settingsView.session.deleteSubspaceButton")}
          </button>
        )}
      </div>
    </div>
  );
}

function IconSection({
  app,
  subspace,
  iconUrl,
  iconPath,
  onChanged,
  onToast,
}: {
  app: WebApp;
  subspace: Subspace;
  iconUrl?: string;
  iconPath?: string;
  onChanged: () => void;
  onToast: ToastFn;
}) {
  const { t } = useTranslation();

  function draftFromProps() {
    return {
      iconPath,
      background: subspace.icon_background_color,
      padding: app.icon_style.padding_percent,
    };
  }

  const [draft, setDraft] = useState(draftFromProps);
  const [draftIconUrl, setDraftIconUrl] = useState(iconUrl);

  useEffect(() => {
    setDraft(draftFromProps());
    setDraftIconUrl(iconUrl);
  }, [subspace.id, app.icon_style.padding_percent]);

  const isDirty =
    draft.iconPath !== iconPath ||
    draft.background !== subspace.icon_background_color ||
    draft.padding !== app.icon_style.padding_percent;

  function cancel() {
    setDraft(draftFromProps());
    setDraftIconUrl(iconUrl);
  }

  async function onIconResolved(path: string) {
    setDraft((d) => ({ ...d, iconPath: path }));
    setDraftIconUrl(await api.readIconDataUrl(path));
  }

  function commitPadding(value: number) {
    const clamped = Math.max(0, Math.min(40, Math.round(value)));
    setDraft((d) => ({ ...d, padding: clamped }));
  }

  async function save() {
    try {
      if (draft.iconPath && draft.iconPath !== iconPath) {
        await api.setSubspaceIcon(app.id, subspace.id, draft.iconPath);
      }
      if (draft.background !== subspace.icon_background_color) {
        await api.setSubspaceIconBackground(app.id, subspace.id, draft.background);
      }
      if (draft.padding !== app.icon_style.padding_percent) {
        await api.setAppIconStyle(app.id, app.icon_style.fit, app.icon_style.rounded, draft.padding);
      }
      onChanged();
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  return (
    <div className="settings-section">
      <h2>{t("settingsView.icon.title")}</h2>

      <IconPicker
        iconUrl={draftIconUrl}
        iconPath={draft.iconPath}
        rounded={app.icon_style.rounded}
        fit={app.icon_style.fit}
        paddingPercent={draft.padding}
        background={draft.background ?? app.icon_background_color}
        fallbackChar={subspace.name.charAt(0).toUpperCase()}
        searchSeed={subspace.name}
        onFetchFavicon={() => api.fetchSubspaceFavicon(app.id, subspace.id)}
        onIconResolved={onIconResolved}
      />

      <label className="settings-field settings-field-row">
        <span>{t("settingsView.icon.backgroundColorLabel")}</span>
        <div className="color-row">
          <input
            type="color"
            value={draft.background ?? DEFAULT_ICON_BG}
            onChange={(e) => setDraft((d) => ({ ...d, background: e.target.value }))}
          />
          <button className="ghost" onClick={() => setDraft((d) => ({ ...d, background: null }))}>
            {t("settingsView.icon.noneButton")}
          </button>
        </div>
      </label>

      <label className="settings-field" style={{ marginTop: "1rem" }}>
        <span>{t("settingsView.icon.paddingLabel")}</span>
        <small>{t("settingsView.icon.paddingHint")}</small>
        <span className="icon-padding-row">
          <input
            type="range"
            min={0}
            max={40}
            value={draft.padding}
            onChange={(e) => commitPadding(Number(e.target.value))}
          />
          <input
            type="number"
            min={0}
            max={40}
            value={draft.padding}
            onChange={(e) => commitPadding(Number(e.target.value))}
          />
          <span className="icon-padding-unit">%</span>
        </span>
      </label>

      <small>{t("settingsView.icon.shapeHint")}</small>

      {isDirty && <SaveActions onSave={save} onCancel={cancel} />}
    </div>
  );
}
