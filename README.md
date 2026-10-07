# try-the-new-way

Monorepo con:

- `be/` — backend in Rust (esporrà le API)
- `fe/` — client web React + Vite (TypeScript) che consuma le API
- `specs/` — registro datato delle specifiche e delle decisioni (vedi `specs/README.md`)

## Avvio

```bash
# Backend
cd be && cargo run

# Frontend (dev server su http://localhost:5173)
cd fe && npm install && npm run dev
```
