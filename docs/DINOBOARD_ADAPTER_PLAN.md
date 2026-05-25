# DinoBoard Adapter Plan

GemHUD v0 ships the browser bridge and a local value API. The Rust native
adapter can now apply a BGA base Splendor public snapshot through DinoBoard's C
ABI before running MCTS.

## Inputs From BGA

The userscript sends:

- visible card DOM summaries
- a mapped `dinoboard_snapshot` from public `gameui.gamedatas` when available
- source URL and client card IDs

The adapter keeps the raw BGA payload out of the scoring core. Rust receives the
normalized snapshot and writes the fields into DinoBoard's schema slots.

## DinoBoard Mapping

The mapper builds a DinoBoard-compatible base Splendor public snapshot:

- bank tokens
- player gems, bonuses, points, card counts, and reserve counts
- public market card IDs
- public nobles
- deck sizes
- current player and pending stage

Then native mode runs DinoBoard analysis with root edge coverage and maps root
action values back to visible cards:

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

Current native mode maps BGA base-card features to DinoBoard internal card ids,
applies the snapshot through low-level C ABI field setters, rebuilds masked AI
views, and maps card slot action ids to DinoBoard root action values.
