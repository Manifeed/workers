# Manifeed Workers

Repo Rust des workers Manifeed. Il regroupe le workspace, le crate partage de config/runtime,
les deux executables metier et l'application desktop partagee.

## Workspace

- `manifeed-worker-common/` : config persistante, status files, version check, services utilisateur, auth et client backend partages
- `worker-rss/` : worker RSS natif
- `worker-source-embedding/` : worker d'embeddings
- `worker-source-embedding-desktop/` : application desktop partagee `Manifeed Workers`
- `installers/` : packaging Debian/Ubuntu, packaging macOS et script de release des bundles workers

## Experience utilisateur actuelle

- Linux distribue un seul paquet `manifeed-workers-desktop_<version>_<arch>.deb` ;
- RSS et Embedding sont telecharges, installes, mis a jour et supprimes directement depuis l'application desktop ;
- l'installation nominale ne demande plus que `api_url` et `api_key` ;
- la configuration locale persistante est stockee dans `workers.json` ;
- l'application `manifeed-workers` expose :
  - une page `Installer`
  - `Scraping`
  - `Embedding`
- la page `Installer` gere les bundles locaux et les mises a jour par worker ;
- les pages `Scraping` et `Embedding` permettent de modifier la configuration, de tester la connexion,
  de lire le statut local, de voir les chemins runtime, et de piloter le worker ;
- le mode `Manuel` lance un processus a la demande depuis l'application ou le CLI ;
- le mode `Service utilisateur` installe un service OS qui peut continuer sans garder l'application ouverte.
- une mise a jour ou une desinstallation est refusee tant que le worker cible est en cours d'execution ;
- un bundle desktop n'est installe que si le manifeste fournit un `sha256` valide ;
- si un `status.json` local est partiellement ecrit ou invalide, l'app conserve le dernier etat exploitable
  et affiche une notice warning au lieu d'effacer le snapshot.

## Commandes utiles

```bash
cargo test -p worker-rss
cargo test -p worker-source-embedding
cargo build --release -p worker-rss
cargo build --release -p worker-source-embedding -p worker-source-embedding-desktop
./installers/release-workers.sh --family desktop
./installers/release-workers.sh --family rss
./installers/release-workers.sh --family embedding
./installers/release-workers.sh --family desktop --family rss --family embedding
./installers/debian/build-debs.sh
```

## Notes d'architecture

- `dist/` est un artefact genere localement et n'est plus versionne.
- `installers/release-workers.sh` publie dans `../backend/var/worker-releases/` et maintient `catalog.json`.
- chaque architecture peut maintenant porter un `latest_version` distinct via les metadata `artifact_version_<platform>_<arch>` dans les `Cargo.toml`, sans changer le `worker_version` backend.
- la configuration nominale des workers est persistante dans `workers.json`, mais les URLs backend et plusieurs timings critiques sont maintenant figes dans les binaires.
- l'app desktop partagee lit les status files locaux RSS/embedding et pilote les deux workers avec deux pages distinctes `Scraping` et `Embedding`.
- le paquet Debian installe uniquement l'app desktop dans `/usr/lib/manifeed/desktop` avec un wrapper `/usr/bin/manifeed-workers`.
- les bundles workers sont extraits dans `~/.local/share/manifeed/<worker>/current`.
- les families `desktop`, `rss` et `embedding` sont publiees independamment ; une release n'ecrase pas les autres familles.
- les bundles, paquets et CLI verifient leur version via le catalogue backend expose par `/workers/releases/manifest`.
- le desktop se telecharge publiquement ; les bundles RSS et Embedding exigent une API key worker valide.
- le worker d'embeddings telecharge et met en cache les artefacts du modele au besoin ; les binaires ONNX locaux du monorepo ne sont donc pas remigres comme sources versionnees.
- `../infra` porte les commandes transverses et la stack locale.
- `../api` publie le contrat backend consomme par les workers.
- le crate desktop documente sa structure et ses notes de release dans
  `worker-source-embedding-desktop/README.md` et `worker-source-embedding-desktop/CHANGELOG.md`.
