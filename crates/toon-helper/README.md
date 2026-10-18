# toon-helper

Shared output helpers for the AXI-compliant CLIs in this workspace. It encodes a value as TOON (Token-Oriented Object Notation) by default, falls back to JSON, and truncates long strings for detail views.

## Purpose

`toon-helper` gives every command one way to print results. `toon_encode` and `format_text` produce TOON (about 40% fewer tokens than JSON, per AXI §1) and switch to pretty JSON when asked. `truncate_str` and `truncate_json_strings` cut long content to a character limit for AXI §3 detail views. The `tdg-rust` binary calls `print_output` and `truncate_json_strings` from `src/main.rs`.

## Configuration

There is no runtime configuration. The format is a plain string argument:

- `"toon"` (any value other than `"json"`): TOON, the default.
- `"json"`: `serde_json::to_string_pretty`.

Its only dependencies are `toon-format` (default features off), `serde` and `serde_json`, declared in `Cargo.toml`.

## Extension points

- `format_text` is the single place where a new output format would be added. Callers compare the format string and fall through to TOON.
- `truncate_json_strings` walks any `serde_json::Value`, so new detail views can reuse it without a schema.
- To add a field to every output, change the serialized value before it reaches `print_output`; the helper does not inject fields.

## Limitations

- TOON encoding can fail for some values. `toon_encode` then returns JSON silently, and a caller that needs to know the encoding used must compare the output itself.
- `truncate_str` counts characters, not display width, so wide glyphs can overflow a terminal column.
- `print_output` writes to stdout with `println!` and cannot return an error.

## Model experience

The default TOON output is shorter for an agent to read, which is the point of the crate. Agents that need exact JSON should pass `"json"`. The crate does not describe its output to the model; the calling command's help text does that.
