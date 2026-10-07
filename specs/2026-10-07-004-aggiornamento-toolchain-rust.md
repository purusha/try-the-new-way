# 004 — Aggiornamento toolchain Rust e configurazione BE

- **Data:** 2026-10-07
- **Stato:** Implementata
- **Ambito:** BE
- **Sostituisce / correlata a:** sostituisce la decisione "Toolchain Rust" di 003 e ne chiude il relativo punto aperto

## Richiesta originale
> ho aggiornato rust, aggiorna i file di config della parte be

## Decisioni
- La toolchain installata ora è Rust 1.99.0 stable (aggiornata dall'utente).
- In `be/Cargo.toml`:
  - `rust-version` passa da `1.84` a `1.99`;
  - `edition` passa da `2021` a `2024`, l'edition attuale, che è anche quella usata di default da `cargo new`.
- Rimosso `be/.cargo/config.toml`: con l'edition 2024 il resolver v3 sceglie già da solo versioni compatibili con `rust-version`, quindi l'impostazione `incompatible-rust-versions = "fallback"` era ridondante.
- `be/Cargo.lock` rigenerato, così le dipendenze passano alle ultime versioni compatibili.

## Assunzioni
- Il passaggio all'edition 2024 è incluso nella richiesta di aggiornare la configurazione: il codice attuale è minimale e compatibile.

## Punti aperti / rischi
- Con `rust-version = "1.99"`, chi lavora al BE deve avere almeno Rust 1.99.

## Impatto
`be/Cargo.toml`, `be/Cargo.lock`, `be/.cargo/config.toml` (rimosso).
