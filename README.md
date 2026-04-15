# Manifeed Workers

Workspace Rust des workers Manifeed. Il regroupe le crate partage, les deux binaires metier
et l'application desktop qui installe et pilote les workers.

## Workspace

- `manifeed-worker-common/` : config persistante, auth worker, checks de release, status local et client gateway HTTP partage
- `worker-rss/` : worker RSS natif
- `worker-source-embedding/` : worker d'embeddings
- `worker-desktop/` : application desktop partagee `Manifeed Workers`
- `installers/` : packaging Debian/Ubuntu, packaging macOS et outillage de release

## Flux runtime

Les workers metier suivent maintenant un flux unique :

1. ouvrir une `worker_session`
2. `claim` une ou plusieurs `worker_tasks`
3. executer la task localement
4. envoyer `complete` ou `fail`
5. nettoyer le state local uniquement apres ack backend

Points cle :

- le crate `manifeed-worker-common` porte le client gateway partage pour `sessions/open`, `tasks/claim`, `tasks/complete` et `tasks/fail`
- chaque claim backend attribue un `execution_id` distinct du `task_id`
- `complete` et `fail` sont idempotents cote backend pour un retry identique sur une lease deja finalisee
- les workers ne parlent ni a PostgreSQL ni a Qdrant directement
- les status files locaux restent la seule telemetrie runtime partagee avec l'app desktop

## Experience utilisateur

- Linux distribue un seul paquet `manifeed-workers-desktop_<version>_<arch>.deb`
- RSS et Embedding sont telecharges, installes, mis a jour et supprimes depuis l'application desktop
- l'installation nominale demande seulement `api_url` et `api_key`
- la configuration persistante est stockee dans `workers.json`
- l'application `manifeed-workers` expose `Installer`, `Scraping` et `Embedding`
- le mode `Manuel` lance un processus a la demande
- le mode `Service utilisateur` installe un service OS qui continue sans garder l'application ouverte
- une mise a jour ou une desinstallation est refusee tant que le worker cible tourne
- les status files sont ecrits de maniere coalescee pour limiter l'I/O disque sur le hot path

## Commandes utiles

```bash
cargo fmt --all
cargo test -p worker-rss
cargo test -p worker-source-embedding
cargo build --release -p worker-rss
cargo build --release -p worker-source-embedding -p worker-desktop
./installers/release-workers.sh --family desktop
./installers/release-workers.sh --family rss
./installers/release-workers.sh --family embedding
./installers/debian/build-debs.sh
```

## Notes d'architecture

- `dist/` est un artefact genere localement et n'est plus versionne
- `installers/release-workers.sh` publie dans `../backend/var/worker-releases/` et maintient `catalog.json`
- `installers/release/` centralise les helpers manifests/catalogue et les familles `desktop`, `rss`, `embedding`
- chaque architecture peut porter un `artifact_version_<platform>_<arch>` distinct sans changer le `worker_version` backend
- l'app desktop lit les status files locaux RSS/Embedding et pilote les deux workers avec deux pages distinctes
- le paquet Debian installe uniquement l'app desktop dans `/usr/lib/manifeed/desktop` avec un wrapper `/usr/bin/manifeed-workers`
- les bundles workers sont extraits dans `~/.local/share/manifeed/<worker>/current`
- les familles `desktop`, `rss` et `embedding` sont publiees independamment
- les bundles, paquets et CLI verifient leur version via `/workers/api/releases/manifest`
- le desktop se telecharge publiquement ; les bundles RSS et Embedding exigent une API key worker valide
- le worker d'embeddings telecharge et met en cache les artefacts du modele au besoin
- la structure et les notes de release de l'app desktop sont documentees dans `worker-desktop/README.md` et `worker-desktop/CHANGELOG.md`
