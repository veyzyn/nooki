# Nooki

Nooki is a simple, local Minecraft server manager for Windows. It handles the repetitive parts of self-hosting—Java, server files, processes, configuration, worlds, mods, plugins, backups, and logs—without turning your computer into a hosting platform.

> Nooki is in early development. Expect breaking changes until the first stable release.

> **Disclaimer:** everything here is vibecoded with the help of gpt 5.6 sol

## Highlights

- Create and import Vanilla, Paper, Fabric, Forge, and NeoForge servers.
- Install supported Java runtimes and select the right runtime for each server.
- Start, stop, restart, and monitor multiple local servers.
- View live console output, metrics, players, logs, and activity.
- Browse and install Paper plugins, Modrinth mods, and CurseForge mods.
- Create modded servers directly from Modrinth and CurseForge server packs.
- Inspect and manage overworlds, dimensions, seeds, spawn points, borders, time, and weather.
- Create, schedule, restore, and retain local backups.
- Provision isolated local databases through Docker Desktop.
- Use an optional activation-gated Nooki relay address for one running server at a time.
- Manage everything without installing a Nooki companion plugin on the Minecraft server.

The open-source application includes the Nooki relay client, but relay access requires a single-use activation key issued by the service operator. Activation is bound to the installation identity and permits one relayed server at a time; other local servers remain fully usable without a relay address.

## Platform support

Nooki currently targets Windows x64. macOS and Linux are not supported yet.

## Running requirements

- Windows 10 or 11, x64
- Microsoft WebView2 Runtime, which is already present on most current Windows installations
- Enough memory and storage for the Minecraft servers you intend to run
- An internet connection when downloading Java, server software, mods, or plugins
- Docker Desktop only when using Nooki's Databases feature

Java does not need to be installed beforehand. Nooki can detect existing Java installations or download a managed runtime when a server needs one. Minecraft servers, mods, plugins, backups, and every other non-database feature run without Docker.

## Download and run

Nooki is distributed as one portable `Nooki-Windows-x64.exe`; it does not require an installer.

1. Open the repository's **Releases** page.
2. Download `Nooki-Windows-x64.exe` from the newest release.
3. Place it anywhere you want and run it.
4. Create a new server or import an existing server folder.

Development builds are also available from the **Build community edition** workflow under the repository's **Actions** tab. Open a successful run and download its artifact. GitHub may require you to sign in for Actions artifacts, and Windows SmartScreen may warn about unsigned development builds.

To use Databases, install and start Docker Desktop before opening the Databases tab. Nooki creates isolated local containers and persistent volumes; Minecraft itself does not run inside Docker.

## Automated builds

The Windows workflow can be started manually from GitHub Actions. It runs the frontend and Rust checks, builds one portable executable, and keeps the resulting Actions artifact for 14 days. Pushing a tag such as `v0.1.0` publishes the same executable permanently on GitHub Releases.

The community workflow also checks the source tree for private files and accidentally committed build credentials before packaging it.

## Development

### Requirements

- Windows 10 or 11, x64
- [Rust](https://www.rust-lang.org/tools/install) with the MSVC toolchain
- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/)
- Microsoft WebView2
- Docker Desktop only if local databases are needed

### Run from source

```powershell
pnpm install
pnpm tauri dev
```

### Create a release build

```powershell
pnpm tauri build
```

### Run the checks

```powershell
pnpm build
pnpm test
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## Optional build configuration

Paper downloads require an identifying contact in Nooki's User-Agent. CurseForge browsing requires an application API key. These values are read at compile time:

```toml
# .cargo/config.toml — keep this file local
[env]
NOOKI_CONTACT_URL = "your-public-support-url-or-email"
NOOKI_CURSEFORGE_API_KEY = "your-curseforge-api-key"
```

Never commit real credentials. Modrinth works without an API key. If a CurseForge author disables third-party downloads, Nooki opens the official download page and watches the Windows Downloads folder for the selected file.

## Local data

Nooki stores its SQLite database, managed Java runtimes, temporary setup files, and archived log sessions in the Windows app-local-data directory. Minecraft servers and backup archives remain in their configured folders.

Nooki does not require an account or upload server files and backups. When activated relay sharing is in use, Minecraft TCP traffic for the selected running server passes through the Nooki-operated relay.

## Contributing and security

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Please report security issues privately using the process in [SECURITY.md](SECURITY.md), not through a public issue.

Minecraft, Minecraft artwork, and the names and marks of third-party services remain the property of their respective owners. Nooki is an independent project and is not affiliated with or endorsed by Mojang Studios, Microsoft, PaperMC, FabricMC, Forge, NeoForged, Modrinth, CurseForge, or Docker. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## License

Nooki Community is available under the [MIT License](LICENSE).
