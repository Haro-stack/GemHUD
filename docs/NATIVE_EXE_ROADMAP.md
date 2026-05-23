# Native Exe Roadmap

GemHUD should be easy to run for players who do not want to manage a Python
environment. The target distribution is a local executable that starts an HTTP
advisor on `127.0.0.1` and serves the same `/analyze` API used by the
Tampermonkey script.

## What Exists Now

`rust-advisor/` builds a native executable:

```bash
cd rust-advisor
cargo build --release
target\release\gemhud-advisor.exe --addr 127.0.0.1:8787
```

It currently implements the public-card heuristic value service. It is useful
for testing the browser-to-local bridge without starting Python.

## Why ONNX Alone Is Not Enough

DinoBoard's `.onnx` file is only the policy/value neural network. A complete
DinoBoard decision or action-value analysis also needs:

- the Splendor rules engine
- legal action generation
- feature encoding
- hidden-information observation tracking
- MCTS and tail solver
- action id to card-slot mapping

Those pieces currently live in DinoBoard's native C++ engine and Python binding
layer, not inside the ONNX file. A Rust binary that only loads
`splendor_2p.onnx` can run raw tensor inference, but it cannot correctly answer
"what is this BGA card worth?" unless the full DinoBoard state pipeline is also
available.

## Native DinoBoard Options

### Option A: C ABI From DinoBoard C++

Expose a small C ABI from DinoBoard:

```c
void* dinoboard_create_session(const char* game_id, const char* model_path);
int dinoboard_apply_snapshot(void* session, const char* json_snapshot);
int dinoboard_analyze(void* session, const char* json_request, char* out_json, int out_len);
void dinoboard_destroy_session(void* session);
```

Then Rust can load that library and serve GemHUD's HTTP API without Python.

This is the most faithful path because it reuses DinoBoard's actual rules,
feature encoder, MCTS, and ONNX Runtime evaluator.

### Option B: Rewrite Base Splendor Engine In Rust

Reimplement base Splendor rules, feature encoding, MCTS, and ONNX Runtime
inference in Rust. This can produce a pure Rust binary, but it duplicates the
most error-prone logic and can drift from DinoBoard.

### Option C: Package Python Internally

Use PyInstaller or Nuitka to ship the existing Python/FastAPI service as an
`.exe`. This is the shortest packaging path, but it is still Python internally.

## Recommended Path

Use the Rust advisor executable now for low-friction GemHUD practice, then add
Option A when the BGA base Splendor state mapper is validated. Keep the HTTP
API stable so the userscript does not need to change.
