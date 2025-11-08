# Latch Engine Licensing FAQ

## Quick Comparison

| License Type | Latch Engine | MIT/Apache | GPL | Unreal Engine |
|--------------|--------------|------------|-----|---------------|
| Use for free? | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| Study source? | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| Modify privately? | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| Redistribute engine? | ❌ No | ✅ Yes | ✅ Yes (with source) | ❌ No |
| Fork publicly? | ❌ No | ✅ Yes | ✅ Yes | ❌ No |
| Game royalties? | ✅ Free | ✅ Free | ✅ Free | 💰 5% over $1M |
| Game license restrictions? | ✅ None | ✅ None | ⚠️ Must be GPL | ✅ None |

## Common Questions

### Can I use this for commercial games?

**Yes, 100% free.** No royalties, no revenue sharing, no fees. Your games are entirely yours.

### Can I modify the engine?

**Yes, for your own use.** You can modify the source code for your projects, but you cannot redistribute your modified version publicly.

### Why can't I redistribute the engine?

This ensures:
1. **Version consistency**: Everyone gets updates from the official source
2. **Future sustainability**: Enables potential monetization (premium features, cloud services) without competing forks
3. **Support quality**: Users aren't running 30 different outdated forks

### What if I want to contribute improvements?

**Submit a pull request!** Contributions are welcome. By submitting, you grant us rights to include your changes in the official engine.

### Can I bundle the runtime with my game?

**Yes!** You can:
- Include the `latch_runtime` binary with your game
- Modify it as needed for distribution
- Distribute it on any platform (Steam, Itch.io, Epic, etc.)

You just need to include: "Powered by Latch Engine"

### What if the engine gets abandoned?

If development stops, you can:
- Continue using the version you have forever
- Continue shipping games with that version
- Request source code release under a different license

Your games are safe—they're bundled binaries that don't depend on ongoing engine development.

### Can I create a competing game engine based on this code?

**No.** You cannot create a public fork or competing engine. If you want to build your own engine, write it from scratch or use an MIT/Apache licensed codebase.

### How does this compare to Unity's license?

**Better for developers:**
- Unity: Runtime fee introduced in 2023 (later reversed after backlash)
- Latch: No runtime fees, ever. Your games are yours.

**More restrictive on engine:**
- Unity: Closed source (can't see or modify)
- Latch: Source-available (can study and modify privately, but can't redistribute)

### How does this compare to Unreal's license?

**Similar approach:**
- Both: Source-available, free to use
- Both: Cannot redistribute engine
- Unreal: 5% royalty after $1M revenue
- Latch: Zero royalties forever

### How does this compare to Godot's license?

**More restrictive:**
- Godot: MIT license (fully open source, fork freely)
- Latch: Source-available (can't fork publicly)

**Why?** Godot is community-driven. Latch is single-author and may monetize in the future (e.g., hosted multiplayer services, asset store).

### What if I violate the license?

Your rights terminate. You must:
- Stop using the engine for new development
- Delete your copies of the engine source

**However:** You can still distribute games you've already built (the runtime is perpetually licensed with your game).

### Can I get a different license?

Maybe! Contact me for:
- Commercial licenses (e.g., white-label, no attribution)
- Custom terms for large studios
- Educational institution licenses

## Examples

### ✅ Allowed

```
# Download engine from official GitHub
git clone https://github.com/UnbrokenTechnology/latchengine.git

# Build your game
latch build my_game

# Ship your game on Steam (includes runtime)
# Your game's LICENSE: MIT, GPL, proprietary, whatever you want
```

### ❌ Not Allowed

```
# Fork the engine publicly
git clone https://github.com/UnbrokenTechnology/latchengine.git
# Modify it
# Push to your own "BetterLatchEngine" repo ❌

# Redistribute the engine
# Create a "Latch Engine + Extra Features" bundle ❌

# Sell the engine tools
# "Buy my custom Latch Engine build!" ❌
```

### ✅ Contribution Workflow

```
# Fork for contribution (private or public fork for PRs is OK)
git clone https://github.com/UnbrokenTechnology/latchengine.git
cd latchengine
git checkout -b fix-ecs-bug

# Make changes
git commit -m "Fix ECS archetype bug"

# Submit PR to official repo
gh pr create

# If merged, your fix becomes part of the official engine ✅
```

## Legal Enforceability

This license is modeled on:
- **Unreal Engine EULA** (epic games)
- **Business Source License** (used by HashiCorp, CockroachDB)
- **Elastic License 2.0** (used by Elasticsearch)

All are legally enforceable. The key restrictions are:
1. No redistribution of the engine
2. No competing derivative works
3. Games are unrestricted

Courts have upheld similar licenses in:
- Oracle v. Google (API copyrightability)
- VMware v. Hellwig (GPL enforcement)
- Artifex v. Hancom (dual licensing)

## Recommendations for Game Developers

**For solo/indie developers:**
This license is ideal. You get full source access, zero fees, and your games are truly yours.

**For large studios:**
Contact for a commercial license if you need:
- White-label builds (no attribution)
- Guaranteed support SLA
- Indemnification clauses

**For open source advocates:**
If you want a fully open engine, use Godot (MIT), Bevy (MIT/Apache), or Fyrox (MIT). This engine prioritizes sustainability over pure openness.

## Contact

Questions? Email: steven.barnett@unbrokentechnology.com  
Commercial licensing: steven.barnett@unbrokentechnology.com  
GitHub Issues: https://github.com/UnbrokenTechnology/latchengine/issues
