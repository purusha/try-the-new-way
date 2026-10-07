# 005 — Repository remoto su GitHub

- **Data:** 2026-10-07
- **Stato:** Implementata
- **Ambito:** Processo
- **Sostituisce / correlata a:** 003 (git init)

## Richiesta originale
> collega quanto è stato fatto al repo git@github.com:purusha/try-the-new-way.git

## Decisioni
- Remote `origin` = `git@github.com:purusha/try-the-new-way.git` (SSH).
- Branch `main` pubblicato su `origin/main` e impostato come upstream.
- Prima del push è stato verificato che il repository remoto fosse vuoto, per non sovrascrivere nulla.

## Assunzioni
- "Collegare" comprende anche il push dei commit esistenti, non solo la configurazione del remote.

## Punti aperti / rischi
- Visibilità del repository (pubblico/privato): la gestisce l'utente su GitHub. Se è pubblico, anche le specifiche e `CLAUDE.md` sono visibili a tutti.
- Workflow di lavoro sul remoto (push diretto su `main` oppure branch + pull request) da definire.

## Impatto
Configurazione git (`origin`), `specs/`.
