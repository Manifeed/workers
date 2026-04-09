# macOS Workers Packaging

Bootstrap package for `Manifeed Workers.app` on `macOS / Apple Silicon`.

## Goals

- build the shared desktop bootstrap app `manifeed-workers`;
- bundle it as `Manifeed Workers.app`;
- publish worker bundles consumed by the desktop app:
  - `rss_worker_bundle`
  - `embedding_worker_bundle` (`none` and/or `coreml`);
- optionally create a signed / notarized `.dmg` in CI.

## Local build

```bash
./installers/macos/build-app.sh
./installers/macos/build-dmg.sh
```

Pour publier uniquement la famille desktop dans le storage backend et mettre a jour le catalogue :

```bash
./installers/release-workers.sh --family desktop
```

Le desktop reste telechargeable publiquement depuis le backend. Les bundles workers RSS et
Embedding restent proteges par API key worker.

Tant qu'aucun build macOS n'est publie, aucune entree macOS n'a besoin d'etre presente dans le
catalogue de release local.

`release-workers.sh` peut aussi empaqueter les bundles macOS depuis :

- `MANIFEED_MACOS_CPU_RUNTIME_DIR` ou `dist/macos/cpu-runtime`
- `MANIFEED_MACOS_COREML_RUNTIME_DIR` ou `dist/macos/coreml-runtime`

Le script n'essaie pas de builder ONNX Runtime CoreML lui-meme. Il empaquette un runtime deja
prepare dans ces repertoires.

## Required CI env vars for signing

- `APPLE_DEVELOPER_ID`
- `APPLE_TEAM_ID`
- `APPLE_APP_PASSWORD`

The scripts deliberately separate:

1. app bundle assembly;
2. optional codesign;
3. optional notarization;
4. dmg creation.

That keeps local development usable even without Apple credentials.
