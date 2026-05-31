# Testing Strategy — pares-radix

## Three Testing Layers

### Layer 1: Vitest (Component/Unit Tests)

```bash
pnpm run test        # single run
pnpm run test:watch  # watch mode
```

- **Environment:** jsdom
- **Config:** `vitest.config.ts`
- **Tests:** `src/**/*.test.ts`
- **Mocks:** `src/test-mocks/` (e.g. `$app/environment`)

Tests cover:
- Platform bridge (`isTauri()` returns false in browser, all tauri functions are no-ops)
- Praxis store (fact CRUD via `emitFact`/`query`)

### Layer 2: Playwright (E2E Tests)

```bash
pnpm run test:e2e
```

- **Config:** `playwright.config.ts`
- **Tests:** `e2e/*.spec.ts`
- **Web server:** Auto-starts `pnpm run dev` on localhost:5173
- **Browser:** Chromium (headless)

Tests cover:
- Smoke (page loads, no JS errors)
- Navigation (all routes render without 404)
- Canvas page (loads successfully)

### Layer 3: Rust Unit Tests (Tauri Backend)

```bash
cd src-tauri && cargo test
```

Tests cover:
- `WindowStatePayload` serialization roundtrip
- `TrayMenuItem` camelCase serialization
- `AppBootedPayload` includes version field

## Run All Layers

```bash
./scripts/test-gui.sh
```

Runs vitest → playwright → cargo test in sequence, fails fast on any error.
