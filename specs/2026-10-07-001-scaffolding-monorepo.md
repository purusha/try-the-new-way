# 001 — Scaffolding monorepo BE Rust + FE React

- **Data:** 2026-10-07
- **Stato:** Implementata
- **Ambito:** BE+FE
- **Sostituisce / correlata a:** —

## Richiesta originale
> ho bisogno che mi crei lo scaffolding di un progetto in cui ti chiederò di generare un progetto be (per esporre api) ed un proggetto fe (come client web che chiamerà le api)

Risposte alle domande di chiarimento:
- Backend: "il be sarà fatto in rust, il fe in react"
- Frontend: React + Vite (TypeScript)
- Struttura: monorepo con `be/` e `fe/`
- Extra (endpoint di esempio, Docker, test, git init): "nessuno di questi, ti darò specifiche in seguito"

## Decisioni
- `be/` creato con `cargo new be --vcs none` (cargo 1.84.0), senza dipendenze.
- `fe/` creato con il template Vite `react-ts` (Node 22).
- `README.md` alla radice con i comandi di avvio.
- Niente git init, Docker, test o endpoint di esempio.

## Assunzioni
- `--vcs none` per non creare un repository git annidato in `be/`.

## Punti aperti / rischi
- Framework HTTP del BE da scegliere (es. axum + tokio).
- Contratto API BE↔FE (formato, versioning, gestione degli errori) da definire.
- Collegamento FE→BE in sviluppo (proxy Vite oppure CORS sul BE) da decidere.
- Il repository non è sotto git: senza versionamento, lo storico delle specifiche esiste solo come file.

## Impatto
`be/`, `fe/`, `README.md`.
