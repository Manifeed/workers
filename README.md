# Manifeed Workers

Repo Rust des workers Manifeed. Il regroupe le workspace, le crate partage d'authentification et les deux executables metier.

## Workspace

- `manifeed-worker-common/` : auth, identite, types et client backend partages
- `worker-rss/` : worker RSS natif
- `worker-source-embedding/` : worker d'embeddings
- `worker-source-embedding-desktop/` : UI Linux locale du worker d'embeddings
- `installers/` : scripts de bundle et d'installation Linux

## Commandes utiles

```bash
cargo test -p worker-rss
cargo test -p worker-source-embedding
cargo build --release -p worker-rss
cargo build --release -p worker-source-embedding -p worker-source-embedding-desktop
./installers/linux/worker-source-embedding/build-bundle.sh
```

## Notes d'architecture

- `dist/` est un artefact genere localement et n'est plus versionne.
- le worker d'embeddings telecharge et met en cache les artefacts du modele au besoin ; les binaires ONNX locaux du monorepo ne sont donc pas remigres comme sources versionnees.
- `../infra` porte les commandes transverses et la stack locale.
- `../api` publie le contrat backend consomme par les workers.
