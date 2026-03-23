# Manifeed Workers

Repo Rust des workers Manifeed. Il regroupe le workspace, le crate partage de config/runtime,
les deux executables metier et l'application desktop partagee.

## Workspace

- `manifeed-worker-common/` : config persistante, status files, version check, services utilisateur, auth et client backend partages
- `worker-rss/` : worker RSS natif
- `worker-source-embedding/` : worker d'embeddings
- `worker-source-embedding-desktop/` : application desktop partagee `Manifeed Workers`
- `installers/` : bundles Linux `worker-rss` et `worker-source-embedding`

## Experience utilisateur actuelle

- l'installation nominale ne demande que `api_url` et `api_key` ;
- la configuration locale persistante est stockee dans `workers.json` ;
- l'application `manifeed-workers` expose maintenant deux pages separees :
  - `Scraping`
  - `Embedding`
- chaque page permet de modifier la configuration, de tester la connexion, de lire le statut local,
  de voir les chemins runtime, et de piloter le worker ;
- le mode `Manuel` lance un processus a la demande depuis l'application ou le CLI ;
- le mode `Service utilisateur` installe un service OS qui peut continuer sans garder l'application ouverte.

## Commandes utiles

```bash
cargo test -p worker-rss
cargo test -p worker-source-embedding
cargo build --release -p worker-rss
cargo build --release -p worker-source-embedding -p worker-source-embedding-desktop
./installers/linux/worker-rss/build-bundle.sh
./installers/linux/worker-source-embedding/build-bundle.sh
```

## Notes d'architecture

- `dist/` est un artefact genere localement et n'est plus versionne.
- la configuration nominale des workers est persistante dans `workers.json`; les env vars restent des overrides experts.
- l'app desktop partagee lit les status files locaux RSS/embedding et pilote les deux workers avec deux pages distinctes `Scraping` et `Embedding`.
- les bundles et CLI verifient leur version via le manifest backend `/workers/releases/manifest`.
- le worker d'embeddings telecharge et met en cache les artefacts du modele au besoin ; les binaires ONNX locaux du monorepo ne sont donc pas remigres comme sources versionnees.
- `../infra` porte les commandes transverses et la stack locale.
- `../api` publie le contrat backend consomme par les workers.
