# Self-update — design

Data: 2026-10-20

## Obiettivo

Silos è un singolo `silos.exe` portable pubblicato come asset di una GitHub
Release (`globdug/silos`). Serve un aggiornamento che avvisa, chiede conferma,
sostituisce l'exe e riapre le app che erano aperte. `tauri-plugin-updater`
non va bene: su Windows richiede installer NSIS/MSI.

## Requisiti concordati

- Controllo all'avvio e ogni 6 ore; opzione globale "Controlla
  automaticamente" (default attiva). Build debug non controllano.
- **Avvisa e chiedi**: banner in dashboard ("Aggiorna ora" / "Dopo" / note di
  rilascio). Nulla viene installato senza click.
- **Pulsante manuale** "Controlla aggiornamenti" nelle impostazioni globali e
  nel footer della dashboard; mostra anche gli errori.
- Verifica integrità con **SHA-256** pubblicato dalla CI
  (`silos.exe.sha256` nella release).
- All'aggiornamento tutte le app (e i loro subspace) vengono chiuse; al
  riavvio **si riaprono quelle che erano aperte**, comprese quelle nascoste in
  tray (restano nascoste) e la dashboard se era aperta.

## Design

### Backend (`src-tauri/src/updater.rs`)

- `UpdateInfo { version, notes_url }` (Serialize).
- Puri e testati: `is_newer(current, latest) -> bool` (semver `x.y.z`,
  prefisso `v` ignorato), `pick_update(release_json, current) -> Option<Asset
  URLs + info>` (cerca asset `silos.exe` e `silos.exe.sha256`, ignora draft e
  prerelease), `verify_sha256(bytes, sha_text) -> bool` (accetta `hash` o
  `hash  silos.exe`, case-insensitive).
- `check_for_update` (comando): GET
  `https://api.github.com/repos/globdug/silos/releases/latest` (header
  `User-Agent`), ritorna `Option<UpdateInfo>`.
- `install_update` (comando async): **rifà il fetch** della release (non si
  fida di URL arrivati dal frontend), scarica l'exe con eventi
  `update-progress {downloaded, total}`, scarica e verifica l'hash (se non
  corrisponde: errore, nessuna modifica). Poi: salva l'elenco delle app aperte,
  le chiude (così `remember_last_tabs_and_clear` salva l'ultima pagina di ogni
  subspace), rinomina `silos.exe` → `silos.old.exe` e il nuovo →
  `silos.exe` (rollback del rename se fallisce e riapertura delle app),
  lancia il nuovo exe con `--updated-from <pid> --reopen <lista>` e chiude
  Silos.
- Avvio con `--updated-from <pid>`: prima di tutto (prima del plugin
  single-instance) attende fino a 10 s che quel processo termini; poi elimina
  `silos.old.exe`. Senza l'attesa il nuovo exe vedrebbe ancora la vecchia
  istanza, le inoltrerebbe i parametri e si chiuderebbe.
- `--reopen` è una lista separata da virgole: `dash`, `<app_id>`,
  `<app_id>:bg` (app nascosta in tray). Le app con PIN si riaprono con la
  schermata di sblocco, come un avvio da shortcut.
- Cartella dell'exe non scrivibile → errore chiaro, nessuna app chiusa.

### Impostazione

- `AppConfig.auto_check_updates: bool` (`#[serde(default = "default_true")]`),
  comandi `get_auto_check_updates` / `set_auto_check_updates`.

### CI

- `release.yml`: dopo la build calcola lo SHA-256 di `silos.exe`, scrive
  `silos.exe.sha256` e lo carica nella release.

### UI

- Banner `UpdateBanner` in dashboard; pulsante e stato in
  `GlobalSettingsView` (con toggle automatico); link "Controlla
  aggiornamenti" nel footer. Testi en/it.

## Fuori portata

Firma crittografica (minisign), aggiornamento in background senza conferma,
rollback automatico dopo un avvio fallito, macOS/Linux.

## Limiti

- Le versioni già installate (≤ 0.2.0) non hanno l'updater: il primo
  aggiornamento si fa a mano.
- L'hash protegge da download corrotti, non da un account GitHub compromesso.
- Il subspace attivo di ogni app non viene ripristinato (si riapre l'ultima
  pagina di ciascun subspace, già salvata dal flusso di chiusura).
