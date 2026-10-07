# 003 — Framework BE axum, proxy Vite, repository git

- **Data:** 2026-10-07
- **Stato:** Implementata (decisione "Toolchain Rust" superata da 004)
- **Ambito:** BE+FE
- **Sostituisce / correlata a:** chiude i punti aperti di 001 e 002; il contratto API è chiuso da 007

## Richiesta originale
> ok axum, proxy vite e fai git init

## Decisioni
- **BE:** framework HTTP axum con runtime tokio. Il server ascolta su `127.0.0.1:3000` e le API sono montate sotto il prefisso `/api`. Per ora il router non ha rotte: gli endpoint arriveranno con le specifiche funzionali.
- **FE:** in sviluppo il dev server di Vite inoltra `/api/*` a `http://localhost:3000`, così il FE chiama URL relativi (`/api/...`) senza dover gestire CORS.
- **Git:** repository inizializzato alla radice del monorepo, con branch `main`. È stato aggiunto un `.gitignore` alla radice per escludere `be/target/`, perché `cargo new --vcs none` non ne crea uno. `fe/` ha già il suo `.gitignore` di Vite.
- **Toolchain Rust:** sulla macchina c'è Rust 1.84, mentre le ultime versioni di alcune dipendenze (es. `hyper-util` 0.1.21) richiedono la 1.85 (edition 2024). Per non aggiornare la toolchain di sistema, `be/Cargo.toml` dichiara `rust-version = "1.84"` e `be/.cargo/config.toml` imposta `incompatible-rust-versions = "fallback"`: cargo sceglie in automatico versioni compatibili con la 1.84.

## Assunzioni
- Prefisso `/api` e porta `3000` scelti come convenzione: servono per far combaciare proxy e BE e si possono cambiare.
- Il proxy vale solo in sviluppo. In produzione il modo in cui FE e BE vengono serviti (stesso dominio con reverse proxy, oppure domini diversi con CORS) è ancora da decidere.

## Punti aperti / rischi
- **Contratto API BE↔FE ancora da definire:** formato delle risposte, formato degli errori, versioning (es. `/api/v1`). Conviene fissarlo prima del primo endpoint, altrimenti ogni endpoint rischia di avere un formato diverso.
- Deploy in produzione (vedi Assunzioni).
- **Toolchain datata:** restare sulla 1.84 blocca su versioni meno recenti delle dipendenze e, con il tempo, alcune crate potrebbero non avere più versioni compatibili. Aggiornare con `rustup update stable` è consigliato; poi basta alzare `rust-version`. Va deciso dall'utente.
- Porta e indirizzo del BE sono fissi nel codice: quando servirà, si passerà a configurarli tramite variabili d'ambiente.

## Impatto
`be/Cargo.toml`, `be/Cargo.lock`, `be/.cargo/config.toml`, `be/src/main.rs`, `fe/vite.config.ts`, `.gitignore`, `.git/`.
