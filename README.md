# Manifeed Workers

Workspace Rust du crawler RSS Manifeed. Le worker `crawler_rss` est autonome et porte son CLI,
son gateway HTTP, sa config locale et son pipeline d'execution.

## Workspace

- `crawler_rss/` : crawler RSS natif, utilisable en CLI
- `installers/` : outillage de release des bundles de crawlers

## Flux runtime

Le crawler RSS suit le flux gateway :

1. ouvrir une `worker_session`
2. `claim` une ou plusieurs `worker_tasks`
3. executer la task localement
4. envoyer `complete` ou `fail`
5. nettoyer le state local uniquement apres ack backend

Points cle :

- `crawler_rss` porte localement le client gateway pour `sessions/open`, `tasks/claim`, `tasks/complete` et `tasks/fail`
- chaque claim backend attribue un `execution_id` distinct du `task_id`
- `complete` et `fail` sont idempotents cote backend pour un retry identique sur une lease deja finalisee
- les workers ne parlent ni a PostgreSQL ni a Qdrant directement
- les status files locaux restent une telemetrie optionnelle pour le diagnostic CLI

## Experience utilisateur

- `crawler_rss` se lance directement en CLI avec `crawler_rss run`
- `crawler_rss set --url ... --api-key ... --concurrency ...` initialise ou met a jour la config locale
- `crawler_rss update` installe la derniere release GitHub compatible
- l'installation nominale demande seulement `url`, `api_key` et `concurrency`
- la configuration persistante du crawler est stockee dans `crawler_rss.json`
- les status files sont ecrits de maniere coalescee pour limiter l'I/O disque sur le hot path
- l'embedding n'est plus un worker Rust ; il passe par `embedding_indexer_service`

## Commandes utiles

```bash
cargo fmt --all
cargo clippy -p crawler_rss --release --all-targets
cargo test -p crawler_rss
cargo build --release -p crawler_rss
./installers/release-workers.sh --family rss
```

## Notes d'architecture

- `dist/` est un artefact genere localement et n'est plus versionne
- `installers/release-workers.sh` peut encore publier un miroir local sous `../worker_service/var/worker-releases/` (outil interne optionnel ; la prod consomme GitHub)
- `installers/release/` centralise les helpers manifests/catalogue et la famille `rss`
- chaque architecture peut porter un `artifact_version_<platform>_<arch>` distinct sans changer le `worker_version` backend
- les bundles workers sont extraits dans `~/.local/share/manifeed/<worker>/current`
- la famille `rss` publie `crawler_rss_bundle`
- les bundles, paquets et CLI verifient leur version via l'API GitHub `releases/latest` du depot `Manifeed/workers`
- le telechargement du bundle GitHub est public ; `crawler_rss run` exige une cle API worker `rss_scrapper` valide cote gateway

## Pipeline de release GitHub

Le workflow `.github/workflows/release.yml` produit les bundles publics consommes par `crawler_rss update`.

- declenche : push d'un tag `v*` ou `workflow_dispatch`
- matrice : linux x86_64, linux aarch64, linux armv7, macos x86_64, macos aarch64, windows x86_64
- naming des artefacts : `crawler_rss_bundle-<version>-<platform>-<arch>.tar.gz` plus le `.sha256` correspondant
- toutes les architectures non natives utilisent `cross` (armv7 uniquement) ; `aarch64` Linux est build sur un runner ARM hoste par GitHub
- chaque tarball contient `bin/crawler_rss[.exe]` strip et un `manifest.json`
- la verification SHA-256 est obligatoire cote client : un release qui oublie le `.sha256` fera echouer `crawler_rss update`

Couverture Raspberry Pi :

| Modele | OS recommande | Cible Rust | Asset |
|---|---|---|---|
| Pi 4 / Pi 5 / Pi 3 (RPi OS 64-bit) | 64-bit | `aarch64-unknown-linux-gnu` | `linux-aarch64` |
| Pi 2 / Pi 3 / Pi Zero 2 (RPi OS 32-bit) | 32-bit (armv7) | `armv7-unknown-linux-gnueabihf` | `linux-arm` |
