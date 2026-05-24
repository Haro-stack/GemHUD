# DinoBoard Adapter Plan

GemHUD v0 ships the browser bridge and a local value API. To use true DinoBoard
MCTS values for BGA base Splendor, the next adapter needs these pieces.

## Inputs From BGA

The userscript already sends:

- visible card DOM summaries
- sanitized public `gamedatas` context when available
- source URL and client card IDs

The adapter must validate the exact BGA base Splendor field names before using
them for model-backed analysis.

## DinoBoard Mapping

The adapter should build a DinoBoard-compatible base Splendor public snapshot:

- bank tokens
- player gems, bonuses, points, card counts, and reserve counts
- public market card IDs
- public nobles
- deck sizes
- current player and pending stage

Then it can run DinoBoard analysis with root edge coverage and map root action
values back to visible cards:

- `buy_faceup` action value -> card buy value
- `reserve_faceup` action value -> card reserve value

The response should keep the same `/analyze` shape used by the userscript.

## Guardrails

- Keep the local service bound to `127.0.0.1`.
- Return values, visit counts, and confidence only.
- Do not expose a move execution endpoint.
- Do not support expansion tables until a separate `splendor_sns_2p` DinoBoard
  rules engine and BGA converter exist.

## Rust Executable Adapter

`rust-advisor/` is the preferred user-facing executable wrapper. It can serve
the GemHUD API without Python and can load DinoBoard's native C ABI DLL through
`--engine dinoboard-native`.

Do not treat `splendor_2p.onnx` as a complete standalone AI. ONNX stores only
the policy/value network. The Rust adapter still needs DinoBoard's rules,
feature encoder, legal action generator, observation tracker, and MCTS to
produce correct card action values.

Current native mode maps card slot action ids to DinoBoard root action values.
Exact live BGA table values still require the snapshot mapper described above.
