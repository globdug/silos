# Gestione download — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Per-app download mode (Download senza chiedere / chiedi ogni volta) e popup nella toolbar con download in corso e conclusi.

**Architecture:** `on_download` di Tauri su ogni tab alimenta un `DownloadRegistry` in memoria (`downloads.rs`); ogni cambio emette l'evento globale `downloads-changed` (payload `app_id`). Toolbar e popup (finestra borderless, come `QuitConfirmPopup`) leggono l'elenco via comandi Tauri.

**Tech Stack:** Rust/Tauri 2.11 (`WebviewBuilder::on_download`, `tauri-plugin-dialog`, `tauri-plugin-opener`), React/TS, i18next (`en.json`, `it.json`).

**Spec:** `docs/superpowers/specs/2026-10-20-downloads-design.md`

## Global Constraints

- `download_mode`: `"downloads"` (default) | `"ask"`; `#[serde(default)]` → nessuna migrazione.
- Solo stato `in_progress` / `completed` / `failed`; nessuna percentuale.
- Elenco solo in memoria; mai sovrascrivere file esistenti (`nome (1).ext`).
- Testi UI in entrambi i file locale (`en.json`, `it.json`); stile da `DESIGN.md`.
- Codice Windows-only non richiesto: tutto via API Tauri portabili.

## Review Focus

- Nome file senza estensione o già esistente → nome univoco, mai sovrascrittura.
- Dialog "Salva con nome" annullato → nessun item in elenco, download non parte.
- Download fallito/interrotto → stato `failed`, nessun panic su `Finished` senza item noto.
- Elenco vuoto → stato vuoto nel popup, icona senza indicatore.
- Download da popup `window.open` → finisce nell'elenco dell'app giusta (verifica manuale).

---

### Task 1: Registry e nome univoco

**Files:**
- Create: `src-tauri/src/downloads.rs`
- Modify: `src-tauri/src/lib.rs` (`mod downloads;`, `app.manage(DownloadRegistry::new())`)

**Interfaces:**
- Produces:
  - `pub enum DownloadStatus { InProgress, Completed, Failed }` (serde `snake_case`)
  - `pub struct DownloadItem { id: String, file_name: String, url: String, path: String, status: DownloadStatus, started_at: i64 }` (Serialize, Clone)
  - `pub struct DownloadRegistry(pub Mutex<HashMap<String, Vec<DownloadItem>>>)` con `new()`, `start(&self, app_id, url, path: &Path) -> String` (ritorna id), `finish(&self, app_id, url, path: Option<&Path>, success: bool)`, `list(&self, app_id) -> Vec<DownloadItem>` (più recenti prima), `clear(&self, app_id)` (rimuove solo item non `InProgress`), `find(&self, app_id, id) -> Option<DownloadItem>`
  - `pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf`

- [ ] **Step 1: Test** in `downloads.rs` `#[cfg(test)]`: `unique_path_appends_counter_when_exists` (crea `a.pdf` in tempdir → `a (1).pdf`, poi `a (2).pdf`), `unique_path_handles_no_extension` (`README` → `README (1)`), `finish_marks_matching_in_progress_item` (start+finish success → `Completed` con path aggiornato), `finish_unknown_url_is_noop`, `clear_keeps_in_progress`.
- [ ] **Step 2:** `cargo test --lib downloads` (da `src-tauri`) → FAIL (modulo mancante).
- [ ] **Step 3:** Implementare i tipi sopra; `finish` cerca l'item `InProgress` più recente con lo stesso `url`; `started_at` = unix secs; `file_name` dal path.
- [ ] **Step 4:** `cargo test --lib downloads` → PASS.
- [ ] **Step 5:** Commit `Add in-memory download registry`.

### Task 2: Campo `download_mode` e comando

**Files:**
- Modify: `src-tauri/src/models.rs` (`WebApp`, `WebAppView`, `From`), `src-tauri/src/commands.rs` (init `WebApp` a ~422; nuovo comando accanto a `set_app_ignore_certificate_errors` ~1437), `src-tauri/src/lib.rs` (handler)

**Interfaces:**
- Produces: `WebApp.download_mode: String` (`#[serde(default = "default_download_mode")]` → `"downloads"`), stesso campo in `WebAppView`; comando `set_app_download_mode(store, app_id: String, mode: String)` che accetta solo `"downloads"`/`"ask"` (altro → ignorato).

- [ ] **Step 1: Test** in `models.rs` `#[cfg(test)]`: `webapp_without_download_mode_deserializes_to_downloads` (JSON minimo senza il campo → `"downloads"`).
- [ ] **Step 2:** `cargo test --lib models` → FAIL.
- [ ] **Step 3:** Aggiungere campo, default fn, `From`, comando; copiare il pattern di `set_app_ignore_certificate_errors`.
- [ ] **Step 4:** `cargo test --lib` → PASS.
- [ ] **Step 5:** Commit `Add per-app download_mode setting`.

### Task 3: Wiring `on_download`, comandi e popup (backend)

**Files:**
- Modify: `src-tauri/src/commands.rs` (builder tab ~1897-1913; nuovi comandi), `src-tauri/src/layout.rs` (`downloads_popup_label`), `src-tauri/src/lib.rs` (handler), `src-tauri/capabilities/default.json` (aggiungere `"downloadspopup-*"` a `windows`)

**Interfaces:**
- Consumes: `DownloadRegistry`, `unique_path`, `WebApp.download_mode`.
- Produces: `layout::downloads_popup_label(app_id) -> String` (`downloadspopup-{id}`); comandi `list_downloads(app_id) -> Vec<DownloadItem>`, `clear_downloads(app_id)`, `open_download(app_id, id)` (opener `open_path`), `reveal_download(app_id, id)` (opener `reveal_item_in_dir`), `open_downloads_popup(app_id)` (come `open_quit_confirm_popup`: ancorato in basso a destra della finestra app, ~340x420, `?downloadsPopupFor={id}`); funzione `fn notify_downloads(app_handle, app_id)` = `app_handle.emit("downloads-changed", app_id)`.

- [ ] **Step 1:** `.on_download(...)` sul `WebviewBuilder` in `open_tab_internal`, con `app_id` clonato. `Requested`: leggere `download_mode` dallo store; `downloads` → `destination = unique_path(app.path().download_dir(), nome dal destination)`; `ask` → `app.dialog().file().set_file_name(..).blocking_save_file()`, `None` → `return false`. Poi `registry.start(...)`, `notify_downloads`, `true`. `Finished` → `registry.finish(...)`, `notify_downloads`, `true`.
- [ ] **Step 2:** Comandi `list/clear/open/reveal/open_downloads_popup` + label + registrazione in `generate_handler!` + capability.
- [ ] **Step 3:** `cargo build` (da `src-tauri`) → compila senza warning nuovi.
- [ ] **Step 4:** Commit `Wire webview downloads into registry and popup commands`.

### Task 4: Frontend

**Files:**
- Modify: `src/types.ts` (`download_mode: "downloads" | "ask"` in `WebApp`; `DownloadItem`), `src/api.ts`, `src/components/AppSettingsView.tsx` (select accanto a "esegui in background", salvataggio come `ignoreCertificateErrors`), `src/components/Toolbar.tsx` (bottone download con indicatore), `src/App.tsx` (`downloadsPopupFor`), `src/App.css`, `src/locales/en.json`, `src/locales/it.json`
- Create: `src/components/DownloadsPopup.tsx`

**Interfaces:**
- Consumes: comandi del Task 3, evento `downloads-changed` (payload `app_id`).
- Produces: `api.listDownloads(appId)`, `api.clearDownloads(appId)`, `api.openDownload(appId, id)`, `api.revealDownload(appId, id)`, `api.openDownloadsPopup(appId)`, `api.setAppDownloadMode(appId, mode)`.

- [ ] **Step 1:** Tipi + `api.ts` + chiavi locale (`toolbar.downloadsTitle`, `downloads.*`: titolo, vuoto, stati, apri, mostra nella cartella, cancella elenco; `appSettings.downloads.*`: etichetta, hint, due opzioni) in en e it.
- [ ] **Step 2:** `DownloadsPopup`: carica elenco, ascolta `downloads-changed` filtrando per `appId`, righe con nome/stato/ora e azioni Apri/Mostra, "Cancella elenco", chiusura su `Escape` e su perdita focus (`getCurrentWindow().onFocusChanged`).
- [ ] **Step 3:** Toolbar: bottone icona download in `toolbar-nav`, puntino se esiste un item `in_progress`; ascolta `downloads-changed`.
- [ ] **Step 4:** AppSettingsView: select `download_mode` con dirty/save/cancel come gli altri campi.
- [ ] **Step 5:** `npm run build` → compila senza errori TS.
- [ ] **Step 6:** Commit `Add downloads popup and per-app download mode UI`.

### Task 5: Verifica e changelog

**Files:**
- Modify: `CHANGELOG.md` (sezione `[Unreleased]` → `### Added`)

- [ ] **Step 1:** `cargo test --lib` e `npm run build` → verdi.
- [ ] **Step 2:** `npm run tauri build -- --no-bundle` → exe in `src-tauri/target/release/silos.exe`.
- [ ] **Step 3:** Aggiungere la voce in CHANGELOG; commit non eseguito senza richiesta.
- [ ] **Step 4:** Test manuale (utente): download in modalità `downloads` e `ask`, annullamento dialog, download fallito, popup `window.open`.
