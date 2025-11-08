Latch Engine — Project Plan (Consolidated Markdown)

A Rust-first, TypeScript-scripted, cross‑platform game engine focused on fast dev iteration, deterministic simulation, and “works everywhere” shipping.

⸻

1) Core Principles
	•	Cross‑platform developer tools: First‑class support on Windows, macOS, and Linux with no compromises or emulation.
	•	Ultra‑fast iteration: Hot reload, near‑instant script turnaround, and low editor overhead.
	•	Ship anywhere: One authoring surface that targets desktop, mobile, web, and consoles.
	•	Runtime performance: 60 FPS minimum on low‑spec ("toaster") hardware via auto‑scaling and fallbacks.
	•	Opinionated defaults: Deterministic sim, strict data contracts, and curated workflows to reduce foot‑guns.
	•	Source control built‑in: Git + LFS + GCM integrated directly into the editor.
	•	Massively multiplayer ready: Unified net model scales from single‑player to MMO without code forks.
	•	Language model: Engine and runtime libraries in Rust; gameplay in TypeScript for most code, Rust for hot paths.
	•	Quake 3 performance target: Games that look like Quake 3 should run as fast as Quake 3 on equivalent hardware—including full CPU fallback with frustum culling, occlusion, and PVS (Potentially Visible Set) for low-poly scenes.

Non‑Goals: Native authoring for cutting‑edge GPU features (e.g., custom HLSL/GLSL pipelines, ray tracing) or translation layers like Wine.

⸻

2) Platform Strategy

2.1 Target Matrix
	•	Development (editor + hot reload): Windows, macOS, Linux.
	•	Shipping: Windows, macOS, Linux, iOS, Android, Web (WebGL), Nintendo Switch, Xbox, PlayStation.

2.2 Dev vs. Ship Policies
	•	Dev builds: Dynamic linking, hot reload, QuickJS execution enabled.
	•	Ship builds: Static (or platform‑approved dynamic) linking. No JIT. Scripting AOT to WASM (or native on permissive platforms). No runtime code loading where restricted.

⸻

3) Architecture Overview

3.1 Engine Core (Rust)
	•	ECS: Data‑oriented (archetype or sparse‑set); POD components; systems scheduled via a job graph.
	•	Jobs: Deadline‑aware scheduler; main thread only for graphics/audio APIs.
	•	Time & Replay: Fixed 60 Hz sim; interpolation for render; input‑only replays for determinism.

3.2 Scripting Model (TypeScript + Rust)
	•	Languages: TS for gameplay; Rust for performance‑critical systems.
	•	Track A (now): Dev with TS→JS (QuickJS) for instant reload; Ship via AssemblyScript→WASM with identical FFI.
	•	Track B (later): TS→SSA compiler targeting native (Cranelift/LLVM) and WASM, drop AS once mature.
	•	Guardrails: Lints enforce an AS‑compatible TS subset for ship path.

3.3 Memory & FFI Contracts
	•	Rust owns memory; scripts handle opaque handles to entities/components/resources.
	•	Access via views (bounded slices/iters) or copy‑out structs; no raw pointers.
	•	Dev (QuickJS): GC allowed within per‑frame budgets; warnings on spikes.
	•	Ship (WASM): Avoid GC on hot paths using arenas/pools; offer stack/arena allocators in stdlib.
	•	Determinism: Fixed timestep, seeded RNG, no wall‑clock/IO in gameplay.

3.4 Hot Reload (Dev only)
	•	Stable C ABI boundary; swap gameplay module (cdylib/WASM) in place; run component schema migrations on reload.

⸻

4) Rendering System

4.1 Goals
	•	Single authoring surface: every exposed feature works across all targets.
	•	Automatic runtime strategy selection based on capability probes.

4.2 Universal Renderer Floor
	•	GPU floor: D3D9 / OpenGL 2.1 / WebGL 1 class.
	•	CPU fallback: SIMD software rasterizer (SSE2/NEON). All platforms can render via GPU or CPU path.
	•	Quake 3 benchmark: Software rasterizer must handle Quake 3-era geometry (5-10k tris/frame) at 60 FPS on period-appropriate hardware (Pentium III / equivalent). Modern low-poly games targeting this aesthetic should run as fast as the original.

4.3 Capability Probes & Strategy Binding
	1.	Probe device/API caps (extensions, MRT count, uniform limits, etc.).
	2.	Choose backend (D3D9/11, GL2.1→3.3, Metal 2+, WebGL 1/2, console SDKs, or software).
	3.	Bind strategy per feature from a ranked table.
	4.	Select assets (formats/mips/precisions) to fit VRAM and caps.
	5.	Auto‑scale continuously to honor frame time/memory budgets.

4.4 Feature Contracts (examples)
	•	Instancing → GPU instancing → fallback: CPU batching.
	•	MRT post‑FX → Single‑pass MRT → fallback: multi‑pass blending.
	•	sRGB textures → HW sRGB → fallback: linear + manual gamma.
	•	Derivatives → dFdx/dFdy → fallback: finite differences.
	•	Texture arrays → HW arrays → fallback: atlases.
	•	Skinning → GPU matrices → fallback: CPU skinning + upload.
	•	Shadows → Depth map → fallback: projected blob/static texture.
	•	Reflections → Static cubemap → fallback: screen‑quad fake.
	•	Particles/PostFX → GPU compute/VBO updates or MRT → fallback: CPU batches/multi‑pass.
	•	Visibility culling → Frustum culling (always) + occlusion queries (GPU) → fallback: PVS (Potentially Visible Set, Quake-style pre-baked room-to-room visibility) for static geometry.

All strategies target visually equivalent results; only performance varies.

4.5 Auto‑Scaler & Budgets
	•	Targets: frame time, VRAM, draw calls.
	•	Controls: LOD bias, shadow map size, MSAA level, particle density, post‑FX scale.
	•	Editor surfaces budget usage and warns when CPU‑raster worst‑case would miss 60 FPS.

4.6 Backends & APIs
	•	D3D9/11 (Windows); GL 2.1→3.3 (cross‑platform); Metal 2+ (macOS/iOS); WebGL 1/2 (Web); console SDKs (NVN/GNM/GXM via vendor integrations); Software rasterizer (fallback).

4.7 Effect IR & Pipeline Generation
	•	Author once in backend‑neutral Effect IR.
	•	Build emits per‑API pipelines and fallback graphs.
	•	Runtime selects the compiled pipeline matching current strategies.

4.8 Editor/Debug Aids
	•	Strategy overlay (“High GPU,” “Web,” “CPU‑only”).
	•	Low‑spec simulation toggles.
	•	Per‑strategy timings and auto‑scaler logs.

⸻

5) Asset Pipeline & DCC

5.1 Authoring Path
	•	Gold path: glTF 2.0 + KTX2 textures → engine binaries.
	•	Import‑only: FBX/COLLADA/OBJ via sandboxed converters (Assimp + optional proprietary SDKs).
	•	Audio: WAV/FLAC → Ogg/ADPCM per target.

5.2 Blender Integration
	•	First‑party add‑on with locked export profile, validation (units/bones/attrs), and one‑click “Export to Project.”
	•	Optionally ship a portable Blender preconfigured for consistency.

5.3 Deterministic Imports
	•	Converters run out‑of‑process; each import produces a manifest with source hashes and normalization notes; incremental re‑import.

⸻

6) Editor & Tooling

6.1 Editor (Rust native)
	•	Dockable UI (egui/ImGui/Qt), scene graph, asset browser, profiler, replay controls, console, compatibility HUD.

6.2 VS Code (Portable)
	•	Ship Code‑OSS portable with pinned Extension Pack: engine LSP, TS/ESLint rules, debugger, Git tools, @engine/api SDK.
	•	Workspace settings/tasks auto‑validated and synced.

6.3 Git & LFS
	•	Bundle Git, Git LFS, and GCM. One‑click GitHub/Bitbucket connect (OAuth).
	•	Auto‑init LFS patterns (*.ktx2, *.wav, *.fbx if stored).
	•	Editor UI for history/branches, LFS lock/unlock, large‑file warnings, asset‑aware diffs.
	•	Support partial clone/sparse checkout for large repos.

⸻

7) Services Layer (Single Mental Model)

Abstracted, capability‑checked services hide platform quirks:
	•	Save: Slots + cloud sync (Steam/PSN/Switch/FS). Quotas & conflict resolution.
	•	Storage/Content: DLC & patch channels; platform layout handling.
	•	Telemetry: Privacy‑gated, buffered, offline queue; store‑specific backends.
	•	Entitlements/Achievements: Unified API; adapters for Steam/Xbox/PSN/Epic/GOG.
	•	Settings: Audio/video/input schemas with per‑platform persistence.
	•	Input: Device abstraction, remapping, recording for replays.
	•	Net: Sockets/HTTP with probes, relays/fallbacks on restricted platforms.
	•	Mods: Desktop‑only WASM plugins/content packs with scoped capabilities.

⸻

8) Modding & Packaging
	•	Packaging: Single binary (where allowed), packed .dat (assets bundled), or loose files (mod‑friendly).
	•	Mods: Optional editor toggle (desktop targets only). Mods load as WASM plugins with capability manifests and sandboxed FS/Net. Hidden/no‑op on restricted platforms.

⸻

9) Build System & CI/CD
	•	Modes: dev (dynamic, symbols, shader cache, QuickJS), ship (LTO, static, baked pipelines, AS→WASM or native later).
	•	Shaders/Effects: Bake per‑backend variants with S/L/M compatibility notes and downgrade rules.
	•	CI templates: GitHub/Bitbucket pipelines to cache deps, build editor/game, run tests, upload artifacts, run compat checker & content lints.

⸻

10) Compatibility & Quality Gates
	•	Project ceiling enforcement: Editor hides disallowed features; asset/material badges show S/L/M support. CI compat checker fails builds exceeding ceilings.
	•	Performance budgets: Per‑frame CPU allocations, draw calls, texture memory. Editor warnings; CI fails on sustained breaches.
	•	Testing: Golden replay tests across S/L/M; soak tests for memory/GC; platform adapter smoke tests for saves/entitlements/achievements (desktop simulators; console via partner CI).

⸻

11) Networking, Simulation & Determinism

11.1 Simulation Model
	•	Tick: 60 Hz fixed (16.666 ms). Render runs as fast as possible with interpolation.
	•	Catch‑up: At most one extra substep; beyond that, slight time dilation to avoid spiral‑of‑death.
	•	Input: Sampled each render frame; visually predicted immediately, applied authoritatively on next tick.
	•	Determinism rules: No wall‑clock reads; deterministic RNG; stable ECS iteration; platform‑stable math.

11.2 Prediction & Correction
	•	Prediction always on; divergences blended out over 2–3 frames. Average input‑to‑action latency ≈ 8 ms (half a tick).

11.3 Work Queue
	•	~1.5 ms/frame budget for background work (pathfinding/streaming/AI). Jobs yield via resume tokens. Deterministic jobs run on snapshots and commit at next tick.

11.4 Rollback Networking (Default)
	•	Philosophy: Single model for solo, co‑op, P2P, and MMO.
	•	Defaults: 60 Hz tick; 2‑tick input buffer (~33 ms); 8‑tick rollback (~133 ms); server‑authoritative; UDP/QUIC with reliable channels.
	•	Flow: Clients predict locally; server runs authoritative sim and sends deltas; clients rollback/resim and blend corrections.
	•	State/Serialization: Per‑archetype ECS pages; per‑tick CRC; fixed math/fixed‑point where needed.
	•	Replication/Interest: Spatial cells + team/party channels; the engine auto‑budgets entities/tick and bytes/tick.
	•	Lag compensation: Server rewind buffer for hit tests; instant local feedback with silent reconciliation.
	•	Security: Server authoritative, mTLS between nodes, per‑session tokens, optional state‑hash challenges.

⸻

12) World Authority & Topology

12.1 Unified Authority
	•	Server always runs—even for single‑player (embedded). Same code path for all modes.
	•	World partitioned into Cells and Combat Bubbles.

12.2 Cells
	•	Fixed spatial regions (≈192 m grid; tunable). One cell = one authoritative sim instance.
	•	Handoff via snapshot → transfer → resume next tick (no dual authority).

12.3 Combat Bubbles
	•	Spawned on engagement; 60 Hz with 8‑tick rollback; placed near latency centroid for participants.
	•	Covers active combatants and immediate environment; merges back after disengage.

12.4 Global Services
	•	Gateway (auth/relay/region routing), Orchestrator (schedules cells/bubbles, failover), Persistence (append‑only event log + periodic snapshots).

12.5 Client Flow
	•	Inputs → server → authoritative sim → deltas → client rollback/resim → visual blend.

12.6 MMO Defaults
	•	Cell size ≈ 128–256 m with 2‑cell hysteresis; bubble size ≈ 2–16 players; ≤30 ms RTT target.

⸻

13) Self‑Organizing Server Architecture

13.1 One Binary
	•	Single headless executable can be leader, worker, or relay; roles are logical and interchangeable.

13.2 Discovery & Auth
	•	LAN: UDP multicast/mDNS. WAN: --join seed.host:7946. Shared join token → auto‑issued mTLS certs.

13.3 Control Plane
	•	Membership via gossip (SWIM/Serf‑style); leader election via embedded Raft (no etcd). Scheduling via consistent hash ring.
	•	Persistence: SQLite (default) or Postgres (flag).

13.4 Lifecycle
	•	Node boots → joins gossip → syncs ring → leader assigns cells/bubbles → automatic reassignment on failure.

13.5 Deployment Modes
	•	Solo/Toaster: node --solo (leader + worker + relay; SQLite).
	•	Small Cluster: node --join seed on each box.
	•	Large MMO: Many nodes; self‑elected leader + workers; optional relays.

13.6 Kubernetes (Optional)
	•	Watches headless Service SRV/Lease; nodes auto‑join; readiness/liveness endpoints; Helm chart. Never required.

13.7 Admin UI & Security
	•	Web console on every node; leader exposes full controls (token auth; disable via flag).
	•	mTLS between nodes with rotating certs. Raft prevents split‑brain; leader re‑election ≈2 s.

13.8 No Lock‑in
	•	Pure executables; works from Raspberry Pi to cloud VMs; embedded gossip + Raft provide self‑organization everywhere.

⸻

14) Consoles & SDKs
	•	No redistribution or automated downloads of console SDKs.
	•	Provide console bridges: build presets, stubs, and adapters that activate when the developer points tooling at a locally installed SDK.
	•	Features disallowed by a platform are handled internally by the Services layer—developers use one mental model.

⸻

15) Security, Privacy, Sandboxing
	•	Script sandbox: WASM with constrained imports and capability tokens for Services.
	•	Mod sandbox: WASM only; no dlopen; signed/unsigned policy toggle (desktop only).
	•	Telemetry: Explicit opt‑in; PII‑safe defaults; offline buffering with size caps.

⸻

16) Documentation & Developer Experience
	•	Starters: “Retro 3D” (S+L), “Low‑poly 3D” (L+M), “2D sprite” (S).
	•	Playbooks: Asset import checklists, determinism best practices, replay debugging, “upgrade L→M” guides.
	•	API docs: Services, ECS handles, Effect IR, Scripting FFI.
	•	Troubleshooting: Import pitfalls, GC spikes, downgrade messages.

⸻

17) Risks & Mitigations
	•	Maintenance surface (formats/tools): Keep one runtime format; sandbox converters; lean on glTF + add‑ons.
	•	Determinism drift: Strict linting; golden replays; CI checks; fixed compiler flags for SIMD.
	•	AS subset friction: Rich stdlib and examples; measure GC; provide arenas; document “perf recipes.”
	•	Web GL differences: Compatibility layer, targeted probes, cross‑browser CI.

⸻

18) Acceptance Criteria (MVP)
	•	Desktop editor with live reload of scripts and Rust gameplay DLL.
	•	ECS + job system stable; Effect IR v1 with downgrade rules.
	•	Asset pipeline (glTF→engine), KTX2 textures; audio import & playback.
	•	Dev scripting in QuickJS; ship scripting via AS→WASM.
	•	Save/Settings Services; deterministic replays.
	•	Portable Code‑OSS + extension pack; integrated Git/LFS UI.
	•	Web build (L path) with S fallback; single bundle; zero extra installs.

⸻

19) Default Project Settings (Recommended)
	•	Scripting: Track A (AS→WASM) with lint profile enabled.
	•	Import: glTF 2.0 + KTX2; FBX/COLLADA allowed as import‑only via converters.
	•	Packaging: Packed .dat + binary; “Enable Mods” off by default.
	•	CI: Compat checker + replay tests + budget checks.

⸻

20) Opinionated Defaults (Summary)

Domain	Default
Sim rate	60 Hz fixed
Input buffer	2 ticks
Rollback window	8 ticks
Prediction	Always on
Physics	Deterministic (locked math mode)
Background work	~1.5 ms/frame budget
Authority model	Server‑authoritative (even SP)
Deployment	One self‑organizing binary
Persistence	SQLite (default) / Postgres (opt)
Scaling	Add nodes; auto‑discover/rebalance
Networking	UDP/QUIC + reliable channels
Security	mTLS cluster; server auth only
Modding	Desktop‑only, sandboxed WASM
Dev mental model	One deterministic tick; small input structs


⸻

21) Glossary (Quick Reference)
	•	Tier S/L/M: Strategy groupings: S = Software/lowest; L = Legacy GPU (GL2.1/D3D9/WebGL1); M = Modern GPU (D3D11/GL3.3/Metal/WebGL2).
	•	Effect IR: Backend‑neutral rendering description compiled into per‑API pipelines.
	•	Cells/Bubbles: World partition and temporary combat instances for scalable authority.
	•	AS: AssemblyScript (TS subset) compiled to WASM.