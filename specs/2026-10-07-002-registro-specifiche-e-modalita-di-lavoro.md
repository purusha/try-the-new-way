# 002 — Registro delle specifiche e modalità di lavoro

- **Data:** 2026-10-07
- **Stato:** Implementata
- **Ambito:** Processo
- **Sostituisce / correlata a:** 001

## Richiesta originale
> prima di procedere con le applicazioni ho una richiesta importante: tutto quello che ti chiedo devi registrarlo in una folder di specifiche. ogni specifica un file nominato anche con la data, in modo che io stesso possa nel tempo andare a vedere la scelte fatte. ho bisogno anche di sapere se creo, durante la fase di scrittura delle specifiche, dei blocchi a livello funzionale ... insomma ho bisogno di avere un partener attivo e non passivo.

## Decisioni
- Cartella `specs/` alla radice del monorepo.
- Nome dei file: `AAAA-MM-GG-NNN-titolo-breve.md`, dove `NNN` è un progressivo globale.
- Template comune in `specs/_TEMPLATE.md`; indice in `specs/README.md`.
- Le specifiche non si riscrivono: una decisione cambiata diventa una nuova specifica, e quella vecchia passa allo stato "Superata da NNN".
- Le regole sono scritte in `CLAUDE.md` alla radice, così valgono in ogni sessione futura.
- Ruolo attivo: prima di implementare, ogni nuova specifica viene confrontata con quelle esistenti. Vanno segnalati contraddizioni, blocchi funzionali e casi non coperti, proposte alternative e poste domande sui punti ambigui.

## Assunzioni
- Per "blocchi a livello funzionale" si intendono incoerenze, dipendenze non soddisfatte o lacune nei requisiti che impedirebbero a una funzionalità di funzionare end-to-end.
- Anche le richieste già fatte (scaffolding) sono state registrate retroattivamente (001).

## Punti aperti / rischi
- Consigliato `git init`: renderebbe tracciabile anche *quando* e *come* sono cambiate le specifiche e il codice. Va deciso dall'utente.

## Impatto
`specs/`, `CLAUDE.md`.
