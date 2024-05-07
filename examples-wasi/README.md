# WASI-Examples
This is a temporary location for the wasi exaples.
These examples should be moved back into the main examples eventually. We'll need to solve the lack of wasi-winit support first.
The examples are a subset of the main examples.

# Run
```bash
cargo build --target wasm32-unknown-unknown --release --package wgpu-examples-wasi --lib --features wasi --no-default-features
wasm-tools component new ./target/wasm32-unknown-unknown/release/wgpu_examples_wasi.wasm -o ./target/examples_component.wasm
cargo +1.77 run --bin wgpu-examples-wasi --features="wasi-runtime" hello_compute
```
