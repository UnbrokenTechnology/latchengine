# Latch Packaging Strategy

## Problem

Developers shouldn't need to:
- Install Rust toolchain
- Cross-compile to other platforms
- Pay for cloud build services
- Own hardware for every target platform

## Solution: Pre-Built Runtimes + Bundler

### Architecture

```
Developer's Project:
├── scripts/
│   └── game.ts          # TypeScript gameplay code
├── assets/
│   ├── models/
│   ├── textures/
│   └── audio/
└── project.toml         # Latch project config

↓ Run: latch package --platform windows

Packaged Game (Windows):
├── game.exe             # Pre-built Latch runtime (from our CI)
├── data/
│   ├── scripts.wasm     # Compiled from game.ts
│   └── assets.dat       # Packed assets
└── game.toml            # Runtime config
```

### Distribution Workflow

#### Step 1: Engine Team (Us) - One-Time Setup

```bash
# We build runtimes via GitHub Actions for:
# - Windows (x64, ARM64)
# - macOS (Universal Binary: x64 + ARM64)
# - Linux (x64, ARM64)
# - Web (WASM)
# - iOS (via Xcode Cloud)
# - Android (via Gradle)

# Outputs stored in GitHub Releases:
# v0.1.0/latch-runtime-windows-x64.zip
# v0.1.0/latch-runtime-macos-universal.zip
# v0.1.0/latch-runtime-linux-x64.tar.gz
# etc.
```

#### Step 2: Developer - Package Command

```bash
# Developer on Windows packaging for macOS:
latch package --platform macos

# What happens:
# 1. Download pre-built macOS runtime from GitHub Releases
# 2. Compile TypeScript → WASM
# 3. Pack assets into .dat file
# 4. Create distributable .app bundle
```

**No Rust compilation. No macOS SDK. Just bundling.**

---

## Implementation

### Packaging Tool (`latch` CLI)

```rust
// crates/latch_cli/src/package.rs

pub struct PackageConfig {
    pub platform: Platform,
    pub project_dir: PathBuf,
    pub output_dir: PathBuf,
}

pub enum Platform {
    WindowsX64,
    WindowsArm64,
    MacOSUniversal,
    LinuxX64,
    LinuxArm64,
    Web,
    IOS,
    Android,
}

pub fn package(config: PackageConfig) -> Result<()> {
    // 1. Download pre-built runtime for target platform
    let runtime_binary = download_runtime(config.platform, ENGINE_VERSION)?;
    
    // 2. Compile scripts
    let compiled_scripts = compile_typescript_to_wasm(&config.project_dir)?;
    
    // 3. Pack assets
    let packed_assets = pack_assets(&config.project_dir)?;
    
    // 4. Bundle everything
    match config.platform {
        Platform::WindowsX64 => create_windows_exe(runtime_binary, compiled_scripts, packed_assets),
        Platform::MacOSUniversal => create_macos_app(runtime_binary, compiled_scripts, packed_assets),
        Platform::LinuxX64 => create_linux_appimage(runtime_binary, compiled_scripts, packed_assets),
        // ...
    }
}

fn download_runtime(platform: Platform, version: &str) -> Result<PathBuf> {
    let url = format!(
        "https://github.com/latchengine/latch/releases/download/v{}/latch-runtime-{}.zip",
        version,
        platform.filename()
    );
    
    // Download, cache locally, verify checksum
    let cache_dir = dirs::cache_dir()?.join("latch").join(version);
    if !cache_dir.exists() {
        download_and_extract(&url, &cache_dir)?;
    }
    
    Ok(cache_dir.join(platform.binary_name()))
}
```

---

## Developer Experience

### Project Structure

```
my_game/
├── latch.toml           # Project config
├── scripts/
│   ├── main.ts
│   └── systems/
│       ├── movement.ts
│       └── combat.ts
├── assets/
│   ├── models/
│   ├── textures/
│   └── audio/
└── .latch/
    └── cache/           # Downloaded runtimes, build artifacts
```

### Commands

```bash
# Run game locally (dev mode)
latch run

# Package for specific platform
latch package --platform windows
latch package --platform macos
latch package --platform linux

# Package for all platforms
latch package --all

# Publish to Steam/Itch.io/etc
latch publish --platform steam
```

---

## GitHub Actions Setup (Engine Team)

```yaml
# .github/workflows/release-runtimes.yml
name: Build Runtime Binaries

on:
  release:
    types: [published]

jobs:
  build-runtimes:
    strategy:
      matrix:
        include:
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact: latch-runtime-windows-x64.zip
          - os: macos-latest
            target: universal  # Use lipo to create fat binary
            artifact: latch-runtime-macos-universal.zip
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact: latch-runtime-linux-x64.tar.gz
    
    runs-on: ${{ matrix.os }}
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Build runtime
        run: |
          cargo build --release -p latch_runtime
          # Strip symbols, compress, etc.
      
      - name: Upload to release
        uses: actions/upload-release-asset@v1
        with:
          upload_url: ${{ github.event.release.upload_url }}
          asset_path: ${{ matrix.artifact }}
          asset_name: ${{ matrix.artifact }}
```

---

## Key Benefits

1. ✅ **No cross-compilation**: Developers never compile Rust
2. ✅ **No SDK requirements**: No Xcode, Visual Studio, etc needed
3. ✅ **Free**: No cloud build credits required
4. ✅ **Fast**: Download pre-built binary (~30 MB) + pack assets
5. ✅ **Deterministic**: Everyone uses exact same runtime binary
6. ✅ **Version pinning**: `latch.toml` specifies engine version

---

## Advanced: Native Rust Gameplay Code

**What if a developer wants to write performance-critical code in Rust?**

```toml
# latch.toml
[engine]
version = "0.1.0"

[native]
enabled = true  # Opt-in to native compilation
modules = ["gameplay"]  # Which Rust modules to compile
```

**Then** they need Rust toolchain + cross-compilation OR use CI/CD:

```yaml
# Developer's own GitHub Actions
- uses: latchengine/build-action@v1
  with:
    platforms: [windows, macos, linux]
    native-modules: true
```

But **most developers won't need this**. TypeScript → WASM is enough for 90% of games.

---

## Comparison to Other Engines

| Engine | Native Code | Cross-Compilation | Cloud Build | Cost |
|--------|-------------|-------------------|-------------|------|
| Unity | C# (IL2CPP) | ❌ Need each platform | Unity Cloud | $9-75/mo |
| Unreal | C++ | ❌ Need each platform | DIY CI/CD | Free* |
| Godot | GDScript/C# | ✅ Via export templates | N/A | Free |
| **Latch** | **TS → WASM** | **✅ Pre-built runtimes** | **GitHub (free)** | **Free** |

*Unreal: Free but requires powerful hardware for compilation

---

## Timeline

### Phase 0 (Current)
- [x] Prove runtime works on macOS
- [ ] CI builds for Windows/Linux

### Phase 1
- [ ] Basic `latch package` command
- [ ] Download pre-built runtimes from GitHub Releases
- [ ] Bundle WASM + assets

### Phase 2
- [ ] Platform-specific packaging (`.app`, `.exe`, AppImage)
- [ ] Asset compression/optimization
- [ ] Icon/metadata embedding

### Phase 3
- [ ] Steam/Itch.io upload integration
- [ ] Code signing (Windows/macOS)
- [ ] Mobile packaging (iOS/Android)
