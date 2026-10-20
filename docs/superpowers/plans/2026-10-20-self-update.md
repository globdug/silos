# Self-update — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox syntax.

**Goal:** Aggiornamento in-app dell'exe portable con conferma, verifica SHA-256 e riapertura delle app aperte.

**Spec:** `docs/superpowers/specs/2026-10-20-self-update-design.md`

## Global Constraints

- Repo `globdug/silos`, asset `silos.exe` + `silos.exe.sha256`; versione corrente da `env!("CARGO_PKG_VERSION")`.
- `install_update` non accetta URL dal frontend; build debug non controllano in automatico.
- Nessun commit/push/tag senza richiesta dell'utente.

## Review Focus

- Hash errato / file hash mancante → nessuna modifica all'exe.
- Tag `v0.10.0` > `v0.9.0` (confronto numerico, non lessicografico).
- Release draft/prerelease o senza asset → nessun aggiornamento.
- Offline / rate limit → controllo automatico silenzioso, manuale con errore.
- Nuovo exe avviato mentre il vecchio è ancora vivo → attende, non si chiude.

### Task 1: `updater.rs` — funzioni pure + test
- [ ] Test: `is_newer` (0.2.0<0.10.0, uguale, prefisso `v`, input non valido), `pick_update` (asset ok, senza sha, draft, prerelease), `verify_sha256` (corretto, maiuscolo, con nome file, sbagliato, vuoto).
- [ ] Implementare con `sha2` (dipendenza) ; `cargo test --lib updater`.

### Task 2: Rete, installazione, riavvio (backend)
- [ ] Comandi `check_for_update`, `install_update`, `get/set_auto_check_updates`; campo `auto_check_updates` in `AppConfig`.
- [ ] `commands.rs`: `pub fn open_apps_snapshot(&AppHandle) -> Vec<String>`, `pub fn close_all_apps(&AppHandle)`, `pub async fn reopen_apps(AppHandle, Vec<String>)`.
- [ ] `lib.rs`: parsing `--updated-from`/`--reopen` all'avvio (attesa PID via `windows` crate, feature `Win32_System_Threading`), reopen invece del dashboard di default.
- [ ] `cargo build`.

### Task 3: CI
- [ ] `release.yml`: step SHA-256 + upload `silos.exe.sha256`.

### Task 4: Frontend
- [ ] `types.ts`/`api.ts`, `UpdateBanner.tsx`, `Dashboard.tsx` (banner + link footer), `GlobalSettingsView.tsx` (pulsante, stato, toggle), locales en/it, CSS.
- [ ] `npx tsc --noEmit`, `npm run build`.

### Task 5: Verifica
- [ ] `cargo test --lib`, build release, CHANGELOG `[Unreleased]`. Test manuale con due release reali.
