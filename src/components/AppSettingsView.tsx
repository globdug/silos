import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AppInfo, WebApp } from "../types";
import { api } from "../api";
import { IconPicker } from "./IconPicker";
import { DEFAULT_ICON_BG } from "../colors";
import { ConfirmTyped } from "./ConfirmTyped";

interface Props {
  appId: string;
}

type Section = "info" | "icon" | "security" | "session" | "details";

type ToastFn = (message: string, type?: "success" | "error") => void;

export function AppSettingsView({ appId }: Props) {
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
  }, [appId]);

  useEffect(() => {
    if (app?.icon) {
      api.readIconDataUrl(app.icon).then(setIconUrl);
    } else {
      setIconUrl(undefined);
    }
  }, [app?.icon]);

  if (!app) return null;

  return (
    <div className="settings-window">
      <nav className="settings-nav">
        <button className={section === "info" ? "active" : ""} onClick={() => setSection("info")}>
          {t("appSettings.nav.info")}
        </button>
        <button className={section === "icon" ? "active" : ""} onClick={() => setSection("icon")}>
          {t("appSettings.nav.icon")}
        </button>
        <button
          className={section === "security" ? "active" : ""}
          onClick={() => setSection("security")}
        >
          {t("appSettings.nav.security")}
        </button>
        <button className={section === "session" ? "active" : ""} onClick={() => setSection("session")}>
          {t("appSettings.nav.session")}
        </button>
        <button className={section === "details" ? "active" : ""} onClick={() => setSection("details")}>
          {t("appSettings.nav.details")}
        </button>
      </nav>

      <div className="settings-panel">
        {section === "info" && <InfoSection app={app} onChanged={refresh} onToast={showToast} />}
        {section === "icon" && <IconSection app={app} iconUrl={iconUrl} onChanged={refresh} onToast={showToast} />}
        {section === "security" && <SecuritySection app={app} onChanged={refresh} onToast={showToast} />}
        {section === "session" && <SessionSection app={app} />}
        {section === "details" && <DetailsSection app={app} />}
      </div>

      {toast && (
        <div className={`toast${toast.type === "error" ? " toast-error" : ""}`} role="status">
          {toast.message}
        </div>
      )}
    </div>
  );
}

function InfoSection({ app, onChanged, onToast }: { app: WebApp; onChanged: () => void; onToast: ToastFn }) {
  const { t } = useTranslation();

  function draftFromApp(a: WebApp) {
    return {
      name: a.name,
      url: a.url,
      runInBackground: a.run_in_background,
      downloadMode: a.download_mode,
      eagerLoadSubspaces: a.eager_load_subspaces,
      hibernateMinutes: String(a.hibernate_delay_secs / 60),
    };
  }

  const [draft, setDraft] = useState(() => draftFromApp(app));

  useEffect(() => {
    setDraft(draftFromApp(app));
  }, [app.id]);

  const isDirty =
    draft.name !== app.name ||
    draft.url !== app.url ||
    draft.runInBackground !== app.run_in_background ||
    draft.downloadMode !== app.download_mode ||
    draft.eagerLoadSubspaces !== app.eager_load_subspaces ||
    draft.hibernateMinutes !== String(app.hibernate_delay_secs / 60);

  function cancel() {
    setDraft(draftFromApp(app));
  }

  async function save() {
    try {
      if (draft.name.trim() && draft.name.trim() !== app.name) {
        await api.renameApp(app.id, draft.name.trim());
      }
      if (draft.url.trim() && draft.url.trim() !== app.url) {
        await api.setAppUrl(app.id, draft.url.trim());
      }
      if (draft.runInBackground !== app.run_in_background) {
        await api.setAppRunInBackground(app.id, draft.runInBackground);
      }
      if (draft.downloadMode !== app.download_mode) {
        await api.setAppDownloadMode(app.id, draft.downloadMode);
      }
      if (draft.eagerLoadSubspaces !== app.eager_load_subspaces) {
        await api.setAppEagerLoadSubspaces(app.id, draft.eagerLoadSubspaces);
      }
      const minutes = Number(draft.hibernateMinutes);
      if (Number.isFinite(minutes) && minutes >= 0) {
        const secs = Math.round(minutes * 60);
        if (secs !== app.hibernate_delay_secs) {
          await api.setAppHibernateDelaySecs(app.id, secs);
        }
      }
      onChanged();
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  return (
    <div className="settings-section">
      <h2>{t("appSettings.info.title")}</h2>

      <label className="settings-field">
        <span>{t("appSettings.info.nameLabel")}</span>
        <input value={draft.name} onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))} />
      </label>

      <label className="settings-field">
        <span>{t("appSettings.info.urlLabel")}</span>
        <input value={draft.url} onChange={(e) => setDraft((d) => ({ ...d, url: e.target.value }))} />
        <small>{t("appSettings.info.urlHint")}</small>
      </label>

      <div className="settings-field">
        <label className="settings-field-row">
          <span>{t("appSettings.info.runInBackgroundLabel")}</span>
          <input
            type="checkbox"
            checked={draft.runInBackground}
            onChange={(e) => setDraft((d) => ({ ...d, runInBackground: e.target.checked }))}
          />
        </label>
        <small>{t("appSettings.info.runInBackgroundHint")}</small>
      </div>

      <label className="settings-field">
        <span>{t("appSettings.info.downloadModeLabel")}</span>
        <select
          value={draft.downloadMode}
          onChange={(e) => setDraft((d) => ({ ...d, downloadMode: e.target.value as "downloads" | "ask" }))}
        >
          <option value="downloads">{t("appSettings.info.downloadModeDownloads")}</option>
          <option value="ask">{t("appSettings.info.downloadModeAsk")}</option>
        </select>
        <small>{t("appSettings.info.downloadModeHint")}</small>
      </label>

      <h3 className="settings-group-heading">{t("appSettings.info.subspaceLoadingHeading")}</h3>

      <div className="settings-field">
        <label className="settings-field-row">
          <span>{t("appSettings.info.eagerLoadLabel")}</span>
          <input
            type="checkbox"
            checked={draft.eagerLoadSubspaces}
            onChange={(e) => setDraft((d) => ({ ...d, eagerLoadSubspaces: e.target.checked }))}
          />
        </label>
        <small>{t("appSettings.info.eagerLoadHint")}</small>
      </div>

      {!draft.eagerLoadSubspaces && (
        <label className="settings-field">
          <span>{t("appSettings.info.hibernateDelayLabel")}</span>
          <input
            type="number"
            min={0}
            step={1}
            value={draft.hibernateMinutes}
            onChange={(e) => setDraft((d) => ({ ...d, hibernateMinutes: e.target.value }))}
          />
          <small>{t("appSettings.info.hibernateDelayHint")}</small>
        </label>
      )}

      {isDirty && (
        <div className="settings-actions">
          <button onClick={save}>{t("common.save")}</button>
          <button className="ghost" onClick={cancel}>
            {t("common.cancel")}
          </button>
        </div>
      )}
    </div>
  );
}

function IconSection({
  app,
  iconUrl,
  onChanged,
  onToast,
}: {
  app: WebApp;
  iconUrl?: string;
  onChanged: () => void;
  onToast: ToastFn;
}) {
  const { t } = useTranslation();

  function draftFromApp(a: WebApp) {
    return {
      iconPath: a.icon,
      background: a.icon_background_color,
      fit: a.icon_style.fit,
      rounded: a.icon_style.rounded,
      padding: a.icon_style.padding_percent,
    };
  }

  const [draft, setDraft] = useState(() => draftFromApp(app));
  const [draftIconUrl, setDraftIconUrl] = useState(iconUrl);

  useEffect(() => {
    setDraft(draftFromApp(app));
    setDraftIconUrl(iconUrl);
  }, [app.id]);

  const isDirty =
    draft.iconPath !== app.icon ||
    draft.background !== app.icon_background_color ||
    draft.fit !== app.icon_style.fit ||
    draft.rounded !== app.icon_style.rounded ||
    draft.padding !== app.icon_style.padding_percent;

  function cancel() {
    setDraft(draftFromApp(app));
    setDraftIconUrl(iconUrl);
  }

  async function onIconResolved(iconPath: string) {
    setDraft((d) => ({ ...d, iconPath }));
    setDraftIconUrl(await api.readIconDataUrl(iconPath));
  }

  function commitPadding(value: number) {
    const clamped = Math.max(0, Math.min(40, Math.round(value)));
    setDraft((d) => ({ ...d, padding: clamped }));
  }

  async function save() {
    try {
      if (draft.iconPath && draft.iconPath !== app.icon) {
        await api.setAppIcon(app.id, draft.iconPath);
      }
      if (draft.background !== app.icon_background_color) {
        await api.setAppIconBackground(app.id, draft.background);
      }
      if (draft.fit !== app.icon_style.fit || draft.rounded !== app.icon_style.rounded || draft.padding !== app.icon_style.padding_percent) {
        await api.setAppIconStyle(app.id, draft.fit, draft.rounded, draft.padding);
      }
      onChanged();
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  return (
    <div className="settings-section">
      <h2>{t("appSettings.icon.title")}</h2>
      <small>{t("appSettings.icon.hint")}</small>

      <div style={{ marginTop: "1rem" }}>
        <IconPicker
          iconUrl={draftIconUrl}
          iconPath={draft.iconPath ?? undefined}
          rounded={draft.rounded}
          fit={draft.fit}
          background={draft.background}
          paddingPercent={draft.padding}
          fallbackChar={app.name.charAt(0).toUpperCase()}
          searchSeed={app.name}
          onFetchFavicon={() => api.fetchAppFavicon(app.id)}
          onIconResolved={onIconResolved}
        />
      </div>

      <label className="settings-field settings-field-row">
        <span>{t("appSettings.icon.backgroundColorLabel")}</span>
        <span className="color-row">
          <input
            type="color"
            value={draft.background ?? DEFAULT_ICON_BG}
            onChange={(e) => setDraft((d) => ({ ...d, background: e.target.value }))}
          />
          <button className="ghost" onClick={() => setDraft((d) => ({ ...d, background: null }))}>
            {t("appSettings.icon.noneButton")}
          </button>
        </span>
      </label>

      <label className="settings-field" style={{ marginTop: "1rem" }}>
        <span>{t("appSettings.icon.fitLabel")}</span>
        <select
          value={draft.fit}
          onChange={(e) => setDraft((d) => ({ ...d, fit: e.target.value as "cover" | "contain" }))}
        >
          <option value="cover">{t("appSettings.icon.fitCover")}</option>
          <option value="contain">{t("appSettings.icon.fitContain")}</option>
        </select>
      </label>

      <label className="settings-field settings-field-row">
        <span>{t("appSettings.icon.bordersLabel")}</span>
        <select
          value={draft.rounded ? "rounded" : "square"}
          onChange={(e) => setDraft((d) => ({ ...d, rounded: e.target.value === "rounded" }))}
        >
          <option value="rounded">{t("appSettings.icon.bordersRounded")}</option>
          <option value="square">{t("appSettings.icon.bordersSquare")}</option>
        </select>
      </label>

      <label className="settings-field" style={{ marginTop: "1rem" }}>
        <span>{t("appSettings.icon.paddingLabel")}</span>
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

      {isDirty && (
        <div className="settings-actions">
          <button onClick={save}>{t("common.save")}</button>
          <button className="ghost" onClick={cancel}>
            {t("common.cancel")}
          </button>
        </div>
      )}
    </div>
  );
}

function SecuritySection({ app, onChanged, onToast }: { app: WebApp; onChanged: () => void; onToast: ToastFn }) {
  const { t } = useTranslation();
  const [pin, setPin] = useState("");
  const [confirmPin, setConfirmPin] = useState("");
  const [error, setError] = useState<string | null>(null);

  function draftFromApp(a: WebApp) {
    return {
      lockOnBackground: a.pin_lock_on_background,
      lockDelaySecs: a.pin_lock_delay_secs,
      ignoreCertificateErrors: a.ignore_certificate_errors,
    };
  }

  const [draft, setDraft] = useState(() => draftFromApp(app));

  useEffect(() => {
    setDraft(draftFromApp(app));
  }, [app.id]);

  const isDirty =
    draft.lockOnBackground !== app.pin_lock_on_background ||
    draft.lockDelaySecs !== app.pin_lock_delay_secs ||
    draft.ignoreCertificateErrors !== app.ignore_certificate_errors;

  function cancel() {
    setDraft(draftFromApp(app));
  }

  async function save() {
    try {
      if (draft.lockOnBackground !== app.pin_lock_on_background || draft.lockDelaySecs !== app.pin_lock_delay_secs) {
        await api.setAppPinLock(app.id, draft.lockOnBackground, draft.lockDelaySecs);
      }
      if (draft.ignoreCertificateErrors !== app.ignore_certificate_errors) {
        await api.setAppIgnoreCertificateErrors(app.id, draft.ignoreCertificateErrors);
      }
      onChanged();
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  const hasPin = app.has_pin;

  async function enablePin() {
    setError(null);
    if (!/^\d{4,8}$/.test(pin)) {
      setError(t("appSettings.security.pinLengthError"));
      return;
    }
    if (pin !== confirmPin) {
      setError(t("appSettings.security.pinMismatchError"));
      return;
    }
    try {
      await api.setAppPin(app.id, pin);
      setPin("");
      setConfirmPin("");
      onChanged();
      onToast(t("common.saved"));
    } catch {
      setError(t("appSettings.security.pinSetError"));
      onToast(t("common.saveError"), "error");
    }
  }

  async function removePin() {
    try {
      await api.setAppPin(app.id, null);
      onChanged();
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  return (
    <div className="settings-section">
      <h2>{t("appSettings.security.title")}</h2>
      <p>{t("appSettings.security.pinIntro")}</p>

      {hasPin ? (
        <>
          <div className="settings-danger-zone">
            <p>{t("appSettings.security.pinActive")}</p>
            <button className="danger-ghost" onClick={removePin}>
              {t("appSettings.security.removePin")}
            </button>
          </div>

          <label className="settings-field settings-field-row" style={{ marginTop: "1rem" }}>
            <span>{t("appSettings.security.lockOnBackgroundLabel")}</span>
            <input
              type="checkbox"
              checked={draft.lockOnBackground}
              onChange={(e) => setDraft((d) => ({ ...d, lockOnBackground: e.target.checked }))}
            />
          </label>

          {draft.lockOnBackground && (
            <label className="settings-field">
              <span>{t("appSettings.security.lockDelayLabel")}</span>
              <select
                value={draft.lockDelaySecs}
                onChange={(e) => setDraft((d) => ({ ...d, lockDelaySecs: Number(e.target.value) }))}
              >
                <option value={0}>{t("appSettings.security.lockDelayImmediate")}</option>
                <option value={60}>{t("appSettings.security.lockDelay1Min")}</option>
                <option value={300}>{t("appSettings.security.lockDelay5Min")}</option>
                <option value={600}>{t("appSettings.security.lockDelay10Min")}</option>
              </select>
            </label>
          )}
        </>
      ) : (
        <>
          <label className="settings-field">
            <span>{t("appSettings.security.newPinLabel")}</span>
            <input
              type="password"
              inputMode="numeric"
              value={pin}
              onChange={(e) => setPin(e.target.value)}
              placeholder={t("appSettings.security.pinPlaceholder")}
            />
          </label>
          <label className="settings-field">
            <span>{t("appSettings.security.confirmPinLabel")}</span>
            <input
              type="password"
              inputMode="numeric"
              value={confirmPin}
              onChange={(e) => setConfirmPin(e.target.value)}
              placeholder={t("appSettings.security.pinPlaceholder")}
            />
          </label>
          {error && <div className="modal-error">{error}</div>}
          <button onClick={enablePin} disabled={!pin || !confirmPin}>
            {t("appSettings.security.enablePinButton")}
          </button>
        </>
      )}

      <h3 className="settings-group-heading">{t("appSettings.security.tlsHeading")}</h3>

      <div className="settings-field">
        <label className="settings-field-row">
          <span>{t("appSettings.security.ignoreCertErrorsLabel")}</span>
          <input
            type="checkbox"
            checked={draft.ignoreCertificateErrors}
            onChange={(e) => setDraft((d) => ({ ...d, ignoreCertificateErrors: e.target.checked }))}
          />
        </label>
        <small>{t("appSettings.security.ignoreCertErrorsHint")}</small>
      </div>

      {isDirty && (
        <div className="settings-actions">
          <button onClick={save}>{t("common.save")}</button>
          <button className="ghost" onClick={cancel}>
            {t("common.cancel")}
          </button>
        </div>
      )}
    </div>
  );
}

function SessionSection({ app }: { app: WebApp }) {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState(false);

  async function reset() {
    await api.resetAppSessions(app.id);
    setConfirming(false);
  }

  return (
    <div className="settings-section">
      <h2>{t("appSettings.session.title")}</h2>
      <p>{t("appSettings.session.hint")}</p>

      <div className="settings-danger-zone">
        <h3>{t("appSettings.session.dangerZone")}</h3>
        {confirming ? (
          <ConfirmTyped label={t("appSettings.session.confirmReset")} onConfirm={reset} onCancel={() => setConfirming(false)} />
        ) : (
          <button className="danger-ghost" onClick={() => setConfirming(true)}>
            {t("appSettings.session.resetAllButton")}
          </button>
        )}
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`;
}

function formatDate(unixSeconds: number, locale: string, unknownLabel: string): string {
  if (!unixSeconds) return unknownLabel;
  return new Date(unixSeconds * 1000).toLocaleDateString(locale, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

function DetailsSection({ app }: { app: WebApp }) {
  const { t, i18n } = useTranslation();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(false);

  async function load() {
    setLoading(true);
    try {
      setInfo(await api.getAppInfo(app.id));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
  }, [app.id]);

  return (
    <div className="settings-section">
      <h2>{t("appSettings.details.title")}</h2>

      <label className="settings-field settings-field-row">
        <span>{t("appSettings.details.createdAtLabel")}</span>
        <span>{formatDate(app.created_at, i18n.language, t("appSettings.details.unknownDate"))}</span>
      </label>

      <label className="settings-field">
        <span>{t("appSettings.details.folderPathLabel")}</span>
        <input readOnly value={info?.folder_path ?? ""} onFocus={(e) => e.currentTarget.select()} />
      </label>

      <label className="settings-field settings-field-row">
        <span>{t("appSettings.details.totalSizeLabel")}</span>
        <span>{loading ? t("appSettings.details.calculating") : info ? formatBytes(info.total_size_bytes) : t("appSettings.details.unknownDate")}</span>
      </label>

      {info && info.subspaces.length > 0 && (
        <div className="settings-field" style={{ marginTop: "1rem" }}>
          <span>{t("appSettings.details.perSubspaceSizeLabel")}</span>
          {info.subspaces.map((s) => (
            <div key={s.id} className="settings-field settings-field-row" style={{ marginBottom: 0 }}>
              <span>{s.name}</span>
              <span>{formatBytes(s.size_bytes)}</span>
            </div>
          ))}
        </div>
      )}

      <button className="ghost" style={{ marginTop: "1rem" }} onClick={load} disabled={loading}>
        {t("appSettings.details.refreshButton")}
      </button>
    </div>
  );
}
