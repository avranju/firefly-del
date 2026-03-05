# Firefly III Transaction Delete By Tag

This is a small Rust based CLI that can delete Firefly III transactions given a
tag string. It retrieves all transactions that have the given tag and deletes
them. It supports a `--dry-run` option that does everything a normal run does
but without actually deleting the transaction.

The Firefly III [OpenAPI spec from version 6.5.1](https://api-docs.firefly-iii.org/firefly-iii-6.5.1-v1.yaml)
was used for building this app.

## Prerequisites

- [Rust](https://rustup.rs/) (edition 2024, Rust 1.85+)
- A running Firefly III instance
- A Firefly III personal access token (Profile → OAuth → Personal Access Tokens)

## Building

```bash
cargo build --release
```

The compiled binary will be at `target/release/firefly-del`.

## Usage

```
firefly-del --url <URL> --token <TOKEN> --tag <TAG> [--dry-run]
```

### Options

| Option            | Description                                                               |
| ----------------- | ------------------------------------------------------------------------- |
| `--url <URL>`     | Base URL of the Firefly III instance (e.g. `https://firefly.example.com`) |
| `--token <TOKEN>` | Personal access token for authentication                                  |
| `--tag <TAG>`     | Tag string to filter transactions by                                      |
| `--dry-run`       | Print transactions that would be deleted without actually deleting them   |

### Examples

Preview which transactions would be deleted (safe, no changes made):

```bash
firefly-del --url https://firefly.example.com --token <TOKEN> --tag groceries --dry-run
```

Delete all transactions tagged `groceries`:

```bash
firefly-del --url https://firefly.example.com --token <TOKEN> --tag groceries
```
