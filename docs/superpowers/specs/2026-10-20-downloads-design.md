# Gestione download per le app — design

Data: 2026-10-20

## Obiettivo

Oggi un download avviato in un'app web finisce nella cartella Download di
Windows senza alcun feedback: l'utente non vede dove va il file e non può
sceglierlo. Si vuole (1) poter scegliere, per ogni app, se salvare
direttamente in Download o chiedere ogni volta dove salvare, e (2) vedere in
una finestra i download in corso e conclusi.

## Requisiti concordati

- Impostazione **per app**: `download_mode` = `downloads` (default, salva in
  Download senza chiedere) oppure `ask` (dialog "Salva con nome").
- Elenco dei download in un **popup dalla toolbar** dell'app, con stato
  `in corso` / `completato` / `fallito`.
- **Solo stato**, nessuna percentuale o velocità (richiederebbe COM WebView2
  specifico per Windows; vedi "Fuori portata").
- Azioni per riga: **Apri** il file, **Mostra nella cartella**.
- Azione globale: **Cancella elenco**.

## Fuori portata

Percentuale di avanzamento, pausa/annullamento di download in corso,
cronologia persistente su disco, vista globale dei download nella dashboard.

## Design

### 1. Backend (`src-tauri/src/downloads.rs`, nuovo)

- Stato in memoria (`DownloadRegistry`, registrato in `lib.rs` come gli altri
  stati): per `app_id`, elenco di `DownloadItem { id, file_name, url, path,
  status, started_at }`. Si azzera alla chiusura di Silos.
- In `open_tab_internal` (`commands.rs`) si aggiunge `.on_download(...)` al
  `WebviewBuilder` di ogni tab (API Tauri 2.11, `DownloadEvent`).
  - `Requested { url, destination }`:
    - modalità `downloads`: `destination` = cartella Download dell'utente +
      nome file univoco (`file (1).pdf` se esiste già, mai sovrascrivere);
    - modalità `ask`: dialog "Salva con nome" (`tauri-plugin-dialog`, già
      presente) proposto in Download; se annullato si ritorna `false` e il
      download non parte e non compare in elenco;
    - in entrambi i casi si registra l'item come `in corso`.
  - `Finished { path, success }`: aggiorna stato (`completato`/`fallito`) e
    percorso reale.
- A ogni cambio si emette l'evento `downloads-changed` verso la toolbar
  dell'app (stesso schema di `notify_toolbar`), senza codice specifico per
  piattaforma.
- Comandi Tauri: `list_downloads(app_id)`, `clear_downloads(app_id)`,
  `open_download(app_id, id)`, `reveal_download(app_id, id)`
  (via `tauri-plugin-opener`), `open_downloads_popup(app_id)`.

### 2. Impostazione per app

- `download_mode` in `models.rs` (`#[serde(default)]` → `downloads`), in
  `types.ts` e in `AppSettingsView.tsx` accanto a "esegui in background".
- Nessuna migrazione: le app esistenti ricevono il default.

### 3. UI

- `Toolbar.tsx`: icona download con indicatore quando ci sono download in
  corso; click → `open_downloads_popup`.
- `DownloadsPopup.tsx` (nuovo) caricato con parametro URL come
  `AddSubspacePopup`/`QuitConfirmPopup` (`?downloadsPopupFor=<appId>` in
  `App.tsx`); finestra borderless creata e posizionata come
  `open_quit_confirm_popup`; label in `layout.rs`.
- Riga: nome, stato, ora, azioni Apri / Mostra nella cartella. Stato vuoto
  quando non ci sono download. Stile da `DESIGN.md`, testi in `src/locales`.

### 4. Test

- Rust: generazione nome univoco; transizioni di stato del registro.
- Manuale: download piccolo, grande, annullato dal dialog, fallito (rete
  interrotta), in entrambe le modalità; download aperto da popup
  `window.open` (rischio sotto).

## Rischi

- `on_download` è per webview: va verificato che i download avviati da
  popup `window.open` arrivino all'elenco dell'app corretta.
- Il popup è una finestra separata sopra la toolbar: va chiuso quando perde il
  focus o cambia la finestra principale, come gli altri popup.
