# Starburst

A native Linux Usenet desktop written in Rust. A quiet charcoal interface, warm amber accents, and one place for your providers, indexers, and downloads.

## Run

Install Nix, then:

```bash
git clone https://github.com/soltros/starburst.git
cd starburst
./dev.sh
```

Or enter `nix-shell` and run `cargo run --locked`. Use a current nixpkgs channel with Rust **1.93 or newer**.

```bash
./build.sh
nix-shell --run ./target/release/starburst
```

The scripts enter the development shell automatically. The release binary uses desktop graphics libraries at runtime; on NixOS run it through the shell as above. No NZBGet, SABnzbd, daemon, web server, or browser is required.

## Getting started

1. Open **Settings**, add your provider hostname, TLS port (usually 563), username, password, and connection allowance. Multiple providers are supported; lower priority values are tried first.
2. Add your Newznab-compatible indexers: display name, base URL (or `/api` endpoint), and API key.
3. Choose a download folder and save. Restart after changing existing connection or folder settings.
4. Use **Discover** to search all indexers and queue a result. Alternatively import an NZB, drop it into the window, or paste an NZB URL.
5. Follow progress in **Downloads**; pause, resume, or cancel a job. Finished and failed jobs appear in **History**.

Automatic post-processing verifies, repairs when possible, and extracts archives. Original archives are retained. PAR2 repair uses `par2`; the development shell provides it along with `7z` for extraction fallback. RAR extraction is also available through the library's bundled native bindings. Providers and indexers require your own accounts.

## Architecture

- **eframe / egui**: native desktop window and interface.
- **usenet-dl 0.4.0**: embedded Rust download manager, SQLite persistence, queue controls and post-processing; NNTP is handled by its `nntp-rs` dependency.
- **reqwest / roxmltree**: Newznab search and NZB retrieval.
- A dedicated Tokio runtime keeps downloads off the UI thread. Indexer searches run in background workers.

Settings and the queue live under `$XDG_CONFIG_HOME/starburst` (normally `~/.config/starburst`). That directory is owner-only, and settings files are written atomically with mode `0600`. Credentials are stored in plaintext in that protected file; system keyring integration is not implemented. Only one instance can open the queue at a time.

## Development

```bash
nix-shell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

This is an initial implementation. It does not yet include streaming playback, RSS automation, password entry for encrypted archives, or desktop notifications. Live provider authentication and downloads require testing with your account. The download library is a young upstream dependency, so report reproducible queue or extraction failures with credentials removed.

## License

GPL-3.0-only; see [LICENSE](LICENSE). Dependencies retain their respective licenses.
