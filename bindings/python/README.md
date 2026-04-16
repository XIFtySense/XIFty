# Python Binding

This is the first Python binding for XIFty.

It is intentionally thin:

- built on top of the `xifty-ffi` C ABI
- implemented with Python's standard-library `ctypes`
- no direct Rust/PyO3 integration in this iteration

## Usage

Build the FFI library first:

```bash
cargo build -p xifty-ffi
```

Then run Python with `bindings/python` on `PYTHONPATH`:

```bash
PYTHONPATH=bindings/python python3 -c "import xifty; print(xifty.version())"
PYTHONPATH=bindings/python python3 -c "import xifty; print(xifty.extract('fixtures/minimal/happy.jpg', view='normalized')['normalized'])"
```

## Tests

```bash
cargo build -p xifty-ffi
PYTHONPATH=bindings/python python3 -m unittest discover -s bindings/python/tests
```
