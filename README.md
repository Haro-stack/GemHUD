# GemHUD

GemHUD is a practice overlay for **Board Game Arena base Splendor**. It reads
public card information from the BGA frontend, sends it to a local-only advisor,
and displays value badges beside visible cards.

GemHUD is intentionally values-only:

- It does not click, submit, or automate BGA actions.
- It does not read hidden decks or private server data.
- It does not send BGA credentials to the local advisor.
- It currently supports **base Splendor only**. Orient, Strongholds, Cities,
  and Sun Never Sets variants are out of scope until their rules and state
  mapping are implemented.

The long-term AI backend target is [Haro-stack/DinoBoard](https://github.com/Haro-stack/DinoBoard).
DinoBoard currently provides the base Splendor AI model and action-value search
foundation used by this project direction.

## Repository Layout

```text
advisor/                 Local FastAPI value service
userscript/              Tampermonkey userscript for BGA
docs/                    Scope, safety, and integration notes
```

## Quick Start

1. Start the local advisor:

   ```bash
   cd advisor
   python -m venv .venv
   .venv\Scripts\activate
   pip install -r requirements.txt
   python -m uvicorn gemhud_advisor.app:app --host 127.0.0.1 --port 8787
   ```

2. Install `userscript/gemhud.user.js` in Tampermonkey.

3. Open a BGA base Splendor table.

4. GemHUD will POST public card data to:

   ```text
   http://127.0.0.1:8787/analyze
   ```

5. The script renders value badges on detected visible cards.

## Current Status

This initial version establishes the browser-to-local-advisor bridge and the
values-only UI guardrails. The default advisor computes public-card feature
values and exposes a stable `/analyze` response shape. A DinoBoard-backed MCTS
adapter can replace the scoring method after the BGA base Splendor public state
mapper is validated against live BGA payloads.

## Advisor API

### `GET /health`

Returns service health and scope metadata.

### `POST /analyze`

Accepts public BGA card information:

```json
{
  "source": "bga",
  "game": "splendor_base",
  "capabilities": {
    "values_only": true,
    "automation": false,
    "base_splendor_only": true
  },
  "cards": [
    {
      "client_id": "dom:card-42",
      "tier": 1,
      "points": 0,
      "bonus_color": "blue",
      "cost": {"white": 1, "green": 2}
    }
  ]
}
```

Returns values by `client_id`:

```json
{
  "ok": true,
  "engine": "gemhud-card-value-v0",
  "game": "splendor_base",
  "scope": "public visible cards; values only; no action automation",
  "cards": [
    {
      "client_id": "dom:card-42",
      "value": 0.52,
      "confidence": 0.8,
      "method": "public-card-heuristic-v0",
      "label": "medium"
    }
  ]
}
```

## Development Notes

- Keep the advisor bound to `127.0.0.1`.
- Do not add endpoints that submit BGA moves.
- Do not log BGA credentials or raw account data.
- Keep expansion support disabled until the DinoBoard expansion rules and BGA
  conversion tests exist.
