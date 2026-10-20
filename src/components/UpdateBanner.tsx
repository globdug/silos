import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Updater } from "../useUpdater";

interface Props {
  updater: Updater;
  onDismiss: () => void;
}

export function UpdateBanner({ updater, onDismiss }: Props) {
  const { t } = useTranslation();
  const { update, status, progress, error } = updater;
  if (!update) return null;

  const installing = status === "installing";

  return (
    <div className="update-banner" role="status">
      <div className="update-banner-text">
        <strong>{t("update.available", { version: update.version })}</strong>
        <small>
          {installing
            ? progress !== null
              ? t("update.downloading", { percent: progress })
              : t("update.installing")
            : t("update.closeNotice")}
        </small>
        {status === "error" && error && <small className="update-banner-error">{t("update.failed", { error })}</small>}
      </div>
      <div className="update-banner-actions">
        <button className="ghost" onClick={() => openUrl(update.notes_url)} disabled={installing}>
          {t("update.notes")}
        </button>
        <button className="ghost" onClick={onDismiss} disabled={installing}>
          {t("update.later")}
        </button>
        <button onClick={updater.install} disabled={installing}>
          {t("update.installNow")}
        </button>
      </div>
    </div>
  );
}
