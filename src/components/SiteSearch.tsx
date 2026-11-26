import { FormEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import type { SiteInfo } from "../types";
import { IconPicker } from "./IconPicker";
import { DEFAULT_ICON_BG } from "../colors";
import { CloseIcon } from "./icons";

interface Resolved {
  name: string;
  url: string;
  iconPath: string | null;
  background: string | null;
  fit: "cover" | "contain";
  rounded: boolean;
  paddingPercent: number;
}

interface Initial {
  name: string;
  url: string;
  icon: string | null;
}

export interface ConfirmResult {
  name: string;
  url: string;
  icon: string | null;
  background: string | null;
  fit: "cover" | "contain";
  rounded: boolean;
  paddingPercent: number;
}

interface Props {
  initial?: Initial;
  onConfirm: (result: ConfirmResult) => void;
  onCancel?: () => void;
  searchPlaceholder?: string;
  confirmLabel?: string;
  /// Shows the icon background-color picker (per-subspace setting).
  showBackground?: boolean;
  /// Starting background color when showBackground is on (e.g. the parent
  /// app's own background, so a new subspace matches it by default).
  defaultBackground?: string | null;
  /// Shows the icon fit/border-shape pickers (app-wide setting).
  showShape?: boolean;
  /// Extra form content rendered above the Cancel/Create buttons (e.g. a
  /// session-group picker for subspaces).
  extraFields?: React.ReactNode;
  /// What the "Favicon del sito" button runs for the current URL. Defaults to
  /// fetching the site over plain HTTP, which can't see pages behind a login.
  fetchFavicon?: (url: string) => Promise<string>;
}

/// Search-bar-first way to add a webapp or subspace: type a name/domain to
/// resolve it into a Name/URL/Icon card (or start pre-filled from
/// `initial`), tweak anything, confirm. The search box stays live so a new
/// search can overwrite the fields at any time, and the icon can be replaced
/// independently (favicon retry or a local file), with optional
/// background/shape controls alongside it.
export function SiteSearch({
  initial,
  onConfirm,
  onCancel,
  searchPlaceholder,
  confirmLabel,
  showBackground = false,
  defaultBackground = DEFAULT_ICON_BG,
  showShape = false,
  extraFields,
  fetchFavicon = api.fetchFaviconForUrl,
}: Props) {
  const { t } = useTranslation();
  const effectiveSearchPlaceholder = searchPlaceholder ?? t("siteSearch.defaultSearchPlaceholder");
  const effectiveConfirmLabel = confirmLabel ?? t("siteSearch.defaultConfirmLabel");
  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resolved, setResolved] = useState<Resolved | null>(
    initial
      ? {
          name: initial.name,
          url: initial.url,
          iconPath: initial.icon,
          background: showBackground ? defaultBackground : null,
          fit: "cover",
          rounded: true,
          paddingPercent: 0,
        }
      : null,
  );
  const [iconUrl, setIconUrl] = useState<string | undefined>();

  useEffect(() => {
    if (resolved?.iconPath) {
      api.readIconDataUrl(resolved.iconPath).then(setIconUrl);
    } else {
      setIconUrl(undefined);
    }
  }, [resolved?.iconPath]);

  async function handleSearch(e: FormEvent) {
    e.preventDefault();
    const trimmed = query.trim();
    if (!trimmed || searching) return;
    setSearching(true);
    setError(null);
    try {
      const info: SiteInfo = await api.resolveSite(trimmed);
      setResolved({
        name: info.title,
        url: info.url,
        iconPath: info.icon_path,
        background: showBackground ? defaultBackground : null,
        fit: "cover",
        rounded: true,
        paddingPercent: 0,
      });
      if (!info.icon_path) {
        setError(t("siteSearch.noIconFoundError"));
      }
    } catch {
      setResolved({
        name: trimmed,
        url: guessUrl(trimmed),
        iconPath: null,
        background: showBackground ? defaultBackground : null,
        fit: "cover",
        rounded: true,
        paddingPercent: 0,
      });
      setError(t("siteSearch.unreachableSiteError"));
    } finally {
      setSearching(false);
    }
  }

  function handleConfirm() {
    if (!resolved || !resolved.name.trim() || !resolved.url.trim()) return;
    onConfirm({
      name: resolved.name.trim(),
      url: resolved.url.trim(),
      icon: resolved.iconPath,
      background: resolved.background,
      fit: resolved.fit,
      rounded: resolved.rounded,
      paddingPercent: resolved.paddingPercent,
    });
    if (!initial) {
      reset();
    }
  }

  function reset() {
    setQuery("");
    setResolved(null);
    setError(null);
  }

  return (
    <div className="site-search-form">
      <form className="site-search" onSubmit={handleSearch}>
        <input
          autoFocus={!initial}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={effectiveSearchPlaceholder}
        />
        <button type="submit" disabled={searching}>
          {searching ? t("siteSearch.searching") : t("siteSearch.searchButton")}
        </button>
        {!initial && (query || resolved) && (
          <button type="button" className="ghost site-search-clear" onClick={reset} title={t("siteSearch.closeTitle")}>
            <CloseIcon />
          </button>
        )}
      </form>

      {error && <div className="site-confirm-error">{error}</div>}

      {resolved && (
        <div className="site-confirm">
          <IconPicker
            iconUrl={iconUrl}
            iconPath={resolved.iconPath ?? undefined}
            rounded={resolved.rounded}
            fit={resolved.fit}
            background={resolved.background}
            paddingPercent={resolved.paddingPercent}
            fallbackChar={resolved.name.charAt(0).toUpperCase() || "?"}
            searchSeed={resolved.name}
            onFetchFavicon={() => fetchFavicon(resolved.url)}
            onIconResolved={(iconPath) => setResolved({ ...resolved, iconPath })}
          />

          {showBackground && (
            <label className="settings-field settings-field-row">
              <span>{t("siteSearch.backgroundColorLabel")}</span>
              <span className="color-row">
                <input
                  type="color"
                  value={resolved.background ?? DEFAULT_ICON_BG}
                  onChange={(e) => setResolved({ ...resolved, background: e.target.value })}
                />
                <button type="button" className="ghost" onClick={() => setResolved({ ...resolved, background: null })}>
                  {t("siteSearch.noneButton")}
                </button>
              </span>
            </label>
          )}

          {showShape && (
            <>
              <label className="settings-field settings-field-row">
                <span>{t("siteSearch.fitLabel")}</span>
                <select
                  value={resolved.fit}
                  onChange={(e) => setResolved({ ...resolved, fit: e.target.value as "cover" | "contain" })}
                >
                  <option value="cover">{t("siteSearch.fitCover")}</option>
                  <option value="contain">{t("siteSearch.fitContain")}</option>
                </select>
              </label>

              <label className="settings-field settings-field-row">
                <span>{t("siteSearch.bordersLabel")}</span>
                <select
                  value={resolved.rounded ? "rounded" : "square"}
                  onChange={(e) => setResolved({ ...resolved, rounded: e.target.value === "rounded" })}
                >
                  <option value="rounded">{t("siteSearch.bordersRounded")}</option>
                  <option value="square">{t("siteSearch.bordersSquare")}</option>
                </select>
              </label>

              <label className="settings-field">
                <span>{t("siteSearch.paddingLabel")}</span>
                <span className="icon-padding-row">
                  <input
                    type="range"
                    min={0}
                    max={40}
                    value={resolved.paddingPercent}
                    onChange={(e) =>
                      setResolved({ ...resolved, paddingPercent: Number(e.target.value) })
                    }
                  />
                  <input
                    type="number"
                    min={0}
                    max={40}
                    value={resolved.paddingPercent}
                    onChange={(e) =>
                      setResolved({
                        ...resolved,
                        paddingPercent: Math.max(0, Math.min(40, Number(e.target.value))),
                      })
                    }
                  />
                  <span className="icon-padding-unit">%</span>
                </span>
              </label>
            </>
          )}

          <label className="settings-field">
            <span>{t("siteSearch.titleLabel")}</span>
            <input value={resolved.name} onChange={(e) => setResolved({ ...resolved, name: e.target.value })} />
          </label>

          <label className="settings-field">
            <span>{t("siteSearch.urlLabel")}</span>
            <input value={resolved.url} onChange={(e) => setResolved({ ...resolved, url: e.target.value })} />
          </label>

          {extraFields}

          <div className="site-search-actions">
            {onCancel && (
              <button type="button" className="ghost" onClick={onCancel}>
                {t("siteSearch.cancel")}
              </button>
            )}
            <button type="button" onClick={handleConfirm}>
              {effectiveConfirmLabel}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function guessUrl(query: string): string {
  if (/^https?:\/\//i.test(query)) return query;
  return query.includes(".") ? `https://${query}` : `https://${query}.com`;
}
