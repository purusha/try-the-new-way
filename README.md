# try-the-new-way

Monorepo con:

- `be/`: backend Rust (axum + sqlx + PostgreSQL), API di gestione magazzino sotto `/api/v1`
- `fe/`: client web React + Vite (TypeScript) che consuma le API
- `api/`: contratto condiviso
  - `openapi.yaml`: OpenAPI 3.1
  - `errors.md`: catalogo degli errori
  - `examples/`: esempi di payload
- `specs/`: registro datato delle specifiche e delle decisioni (vedi `specs/README.md`)

## Avvio in sviluppo

```bash
# 1. Database (Postgres 17 in docker, credenziali di solo sviluppo)
docker compose up -d

# 2. Backend su http://127.0.0.1:3000/api/v1 (le migrazioni partono all'avvio)
cp .env.example be/.env
cd be && cargo run

# 3. Frontend su http://localhost:5173 (il dev server inoltra /api al backend)
cd fe && npm install && npm run dev
```

Nella UI vanno scelti il magazzino e il nome dell'operatore, che viene inviato come header `X-Operator`.

## Test

```bash
cd be && cargo test             # unit test di dominio + test di integrazione (usano DATABASE_URL di be/.env)
cd fe && npm run lint && npm run build
npx @redocly/cli lint --config api/redocly.yaml api/openapi.yaml
```

Dopo una modifica al contratto, rigenerare i tipi del FE con `cd fe && npm run gen:api`.
