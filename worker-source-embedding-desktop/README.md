# Manifeed Workers Desktop

Application desktop partagee `manifeed-workers` pour piloter les workers RSS et Embedding.

## Etat actuel

- un worker en cours d'execution ne peut plus etre mis a jour ou desinstalle depuis l'UI ;
- les bundles telecharges exigent un `sha256` valide et sont verifies en streaming ;
- une desinstallation echouee restaure le bundle local, la config et le service utilisateur ;
- un `status.json` invalide conserve le dernier snapshot exploitable et affiche un warning persistant ;
- les refresh UI sont coalesces pour eviter l'empilement de ticks pendant les operations longues.

## Commandes utiles

Depuis `workers/` :

```bash
cargo check -p worker-source-embedding-desktop --all-targets
cargo test -p worker-source-embedding-desktop
cargo build --release -p worker-source-embedding-desktop
```

## Structure

- `src/controller/` : orchestration UI, snapshots, bindings Slint et cycle de refresh
- `src/installer/` : telechargement, validation, transactions d'installation et de suppression
- `src/process.rs` : verification des processus externes Linux/macOS
- `ui/` : vues Slint, cartes workers et composants communs

## Version

La release courante du crate desktop est `0.1.1`. Les metadata
`artifact_version_linux_x86_64` et `artifact_version_linux_aarch64` dans `Cargo.toml`
pilotent les artefacts Linux publies par `installers/release-workers.sh`.
