# GemHUD

GemHUD is a practice overlay for **base Splendor** on Board Game Arena and the
HullQin ccbs page. It reads public card information from the frontend, sends it
to a local-only advisor, and displays value badges beside visible cards.

GemHUD is intentionally values-only:

- It does not click, submit, or automate game actions.
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
rust-advisor/            Rust executable local value service
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

3. Open a BGA base Splendor table or `https://game.hullqin.cn/ccbs/...`.

4. GemHUD will POST public card data to:

   ```text
   http://127.0.0.1:8787/analyze
   ```

5. The script renders value badges on detected visible cards and shows a
   values-only action suggestion in the panel.

## Rust Advisor Executable

If you do not want to start Python, GemHUD also includes a native Rust advisor:

```bash
cd rust-advisor
cargo build --release
target\release\gemhud-advisor.exe
```

The Rust executable serves the same local values-only API:

```text
GET  /health
POST /analyze
```

With no arguments, the Rust executable listens on `127.0.0.1:8787` and
auto-detects a local DinoBoard checkout at `D:\codex\Haro-DinoBoard`. If the
DinoBoard C ABI DLL and Splendor ONNX model are present, it starts
`dinoboard-native`; otherwise it falls back to the lightweight public-card
heuristic.

You can still override any value with CLI flags:

```bash
target\release\gemhud-advisor.exe \
  --engine dinoboard-native \
  --dinoboard-dll D:\codex\Haro-DinoBoard\build-capi\dinoboard_c_api.dll \
  --model D:\codex\Haro-DinoBoard\games\splendor\model\splendor_2p.onnx \
  --simulations 256
```

For a packaged local setup, place `gemhud-advisor.config.json` next to the
executable or run it from a directory containing that file:

```json
{
  "addr": "127.0.0.1:8787",
  "engine": "dinoboard-native",
  "dinoboard_dll": "D:\\codex\\Haro-DinoBoard\\build-capi\\dinoboard_c_api.dll",
  "model": "D:\\codex\\Haro-DinoBoard\\games\\splendor\\model\\splendor_2p.onnx",
  "simulations": 256,
  "seed": 20260524
}
```

When the userscript can read BGA `gameui.gamedatas` or HullQin ccbs DOM state,
native mode maps the public base Splendor state into DinoBoard before running
MCTS: bank tokens, player tokens and bonuses, visible market cards, nobles, deck
sizes, current player, and public reserve visibility. It still falls back to the
public-card heuristic when the table is an unsupported expansion or a card
cannot be mapped.

## Current Status

The current version establishes the browser-to-local-advisor bridge and the
values-only UI guardrails. The default advisor computes public-card feature
values, while the Rust `dinoboard-native` mode can use a mapped BGA base
Splendor snapshot and DinoBoard MCTS root action values.

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
    "base_splendor_only": true,
    "action_recommendation": true
  },
  "cards": [
    {
      "client_id": "dom:card-42",
      "tier": 1,
      "points": 0,
      "bonus_color": "blue",
      "cost": {"white": 1, "green": 2}
    }
  ],
  "recommendation": {
    "label": "拿宝石 W U G",
    "value": 0.56,
    "confidence": 0.62,
    "method": "state-aware-heuristic-v1"
  }
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
