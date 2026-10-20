import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { changeLanguageEverywhere } from "../i18n";
import { useUpdater } from "../useUpdater";

type Section = "language" | "extensions" | "updates";
type ToastFn = (message: string, type?: "success" | "error") => void;

export function GlobalSettingsView() {
  const { t } = useTranslation();
  const [section, setSection] = useState<Section>("language");
  const [toast, setToast] = useState<{ message: string; type: "success" | "error" } | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  function showToast(message: string, type: "success" | "error" = "success") {
    if (toastTimer.current) clearTimeout(toastTimer.current);
    setToast({ message, type });
    toastTimer.current = setTimeout(() => setToast(null), 2500);
  }

  return (
    <div className="settings-window">
      <nav className="settings-nav">
        <button className={section === "language" ? "active" : ""} onClick={() => setSection("language")}>
          {t("globalSettings.nav.language")}
        </button>
        <button className={section === "extensions" ? "active" : ""} onClick={() => setSection("extensions")}>
          {t("globalSettings.nav.extensions")}
        </button>
        <button className={section === "updates" ? "active" : ""} onClick={() => setSection("updates")}>
          {t("globalSettings.nav.updates")}
        </button>
      </nav>

      <div className="settings-panel">
        {section === "language" && <LanguageSection onToast={showToast} />}
        {section === "extensions" && <ExtensionsSection onToast={showToast} />}
        {section === "updates" && <UpdatesSection />}
      </div>

      {toast && (
        <div className={`toast${toast.type === "error" ? " toast-error" : ""}`} role="status">
          {toast.message}
        </div>
      )}
    </div>
  );
}

function LanguageSection({ onToast }: { onToast: ToastFn }) {
  const { t, i18n } = useTranslation();
  const current = i18n.language.split("-")[0];
  const [draft, setDraft] = useState(current);

  const isDirty = draft !== current;

  function save() {
    try {
      changeLanguageEverywhere(draft);
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  return (
    <div className="settings-section">
      <h2>{t("globalSettings.language.title")}</h2>

      <label className="settings-field settings-field-row">
        <span>{t("globalSettings.language.languageLabel")}</span>
        <select value={draft} onChange={(e) => setDraft(e.target.value)}>
          <option value="it">Italiano</option>
          <option value="en">English</option>
        </select>
      </label>

      {isDirty && (
        <div className="settings-actions">
          <button onClick={save}>{t("common.save")}</button>
          <button className="ghost" onClick={() => setDraft(current)}>
            {t("common.cancel")}
          </button>
        </div>
      )}
    </div>
  );
}

function ExtensionsSection({ onToast }: { onToast: ToastFn }) {
  const { t } = useTranslation();
  const [savedApiKey, setSavedApiKey] = useState("");
  const [apiKey, setApiKey] = useState("");

  useEffect(() => {
    api.getMacosIconsApiKey().then((key) => {
      setSavedApiKey(key ?? "");
      setApiKey(key ?? "");
    });
  }, []);

  const isDirty = apiKey !== savedApiKey;

  async function save() {
    try {
      await api.setMacosIconsApiKey(apiKey.trim() || null);
      setSavedApiKey(apiKey);
      onToast(t("common.saved"));
    } catch {
      onToast(t("common.saveError"), "error");
    }
  }

  function cancel() {
    setApiKey(savedApiKey);
  }

  return (
    <div className="settings-section">
      <h2>{t("globalSettings.extensions.title")}</h2>

      <label className="settings-field">
        <span>{t("globalSettings.extensions.macosIconsApiKeyLabel")}</span>
        <input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder={t("globalSettings.extensions.apiKeyPlaceholder")}
        />
        <small>
          {t("globalSettings.extensions.apiKeyHintPrefix")}{" "}
          <a href="#" onClick={(e) => { e.preventDefault(); openUrl("https://macosicons.com"); }}>
            macosicons.com
          </a>
          {t("globalSettings.extensions.apiKeyHintMiddle")}{" "}
          <a
            href="#"
            onClick={(e) => {
              e.preventDefault();
              openUrl("https://docs.macosicons.com/api-management");
            }}
          >
            docs.macosicons.com
          </a>
          .
        </small>
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

function UpdatesSection() {
  const { t } = useTranslation();
  const updater = useUpdater();
  const [auto, setAuto] = useState(true);

  useEffect(() => {
    api.getAutoUpdateCheck().then(setAuto);
  }, []);

  function toggleAuto(enabled: boolean) {
    setAuto(enabled);
    api.setAutoUpdateCheck(enabled);
  }

  const { update, status, progress, error } = updater;
  const installing = status === "installing";

  return (
    <div className="settings-section">
      <h2>{t("globalSettings.updates.title")}</h2>
      <small>{t("globalSettings.updates.currentVersion", { version: __APP_VERSION__ })}</small>

      <div className="settings-field">
        <label className="settings-field-row">
          <span>{t("globalSettings.updates.autoLabel")}</span>
          <input type="checkbox" checked={auto} onChange={(e) => toggleAuto(e.target.checked)} />
        </label>
        <small>{t("globalSettings.updates.autoHint")}</small>
      </div>

      <div className="settings-actions">
        <button onClick={updater.check} disabled={status === "checking" || installing}>
          {status === "checking" ? t("update.checking") : t("update.checkNow")}
        </button>
        {update && (
          <button onClick={updater.install} disabled={installing}>
            {t("update.installNowVersion", { version: update.version })}
          </button>
        )}
      </div>

      <p className="update-status" role="status">
        {installing
          ? progress !== null
            ? t("update.downloading", { percent: progress })
            : t("update.installing")
          : status === "upToDate"
            ? t("update.upToDate")
            : update
              ? t("update.available", { version: update.version })
              : status === "error" && error
                ? t("update.failed", { error })
                : ""}
      </p>
      {update && !installing && <small>{t("update.closeNotice")}</small>}
    </div>
  );
}
