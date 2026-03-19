# Installeur Linux du worker d'embeddings

Cet installeur cible le bundle Linux du worker `source_embedding`.

## Ce que le bundle contient

- `worker-source-embedding`
- `worker-source-embedding-desktop`
- `install.sh`
- `manifeed-worker-source-embedding.svg`

Le script `build-bundle.sh` copie ces artefacts dans `dist/linux/worker-source-embedding/`.

## Ce que fait `install.sh`

Le script :

1. verifie les outils systeme minimaux ;
2. execute `worker-source-embedding probe` ;
3. choisit un bundle ONNX Runtime adapte a la machine ;
4. installe le runtime ONNX localement ;
5. copie les binaires dans `~/.local/share/manifeed/worker-source-embedding` ;
6. ecrit le fichier d'environnement `~/.config/manifeed/worker-source-embedding.env` ;
7. prepare le fichier d'etat `~/.local/state/manifeed/worker-source-embedding/status.json` ;
8. lance `worker-source-embedding enroll` contre le backend ;
9. installe l'application desktop locale ;
10. peut installer un service `systemd --user` avec `--install-service`.

## Prerequis

- Linux x86_64 ou arm64
- acces reseau au backend Manifeed
- token d'enrolement `source_embedding`
- `curl`, `tar`, `python3`, `sha256sum`

Le script tente d'installer les outils manquants via le gestionnaire de paquets systeme.

## Politique runtime ONNX actuelle

- machine NVIDIA avec driver CUDA utilisable : bundle ONNX Runtime CUDA ;
- autres cas : bundle ONNX Runtime CPU ;
- `webgpu` reste supporte par le binaire, mais l'installeur Linux ne provisionne pas encore
  de runtime partage pour ce backend.

## Construction du bundle

Depuis la racine du repo `workers` :

```bash
./installers/linux/worker-source-embedding/build-bundle.sh
```

## Installation

### Mode interactif CLI

```bash
./dist/linux/worker-source-embedding/install.sh --cli
```

### Mode non interactif

```bash
./dist/linux/worker-source-embedding/install.sh --non-interactive \
  --api-url http://127.0.0.1:8000 \
  --enrollment-token manifeed-embedding-enroll
```

### Installation avec service user

```bash
./dist/linux/worker-source-embedding/install.sh --install-service
```

## Fichiers crees

- application : `~/.local/share/manifeed/worker-source-embedding/`
- environnement : `~/.config/manifeed/worker-source-embedding.env`
- identite : `~/.config/manifeed/worker-source-embedding/`
- cache modeles : `~/.cache/manifeed/worker-source-embedding/models/`
- logs : `~/.cache/manifeed/worker-source-embedding/worker.log`
- status : `~/.local/state/manifeed/worker-source-embedding/status.json`
- lanceur CLI : `~/.local/bin/manifeed-worker-source-embedding`
- desktop entry : `~/.local/share/applications/`

## Options principales

```text
--gui
--cli
--non-interactive
--binary PATH
--desktop-binary PATH
--install-dir PATH
--api-url URL
--enrollment-token TOKEN
--hf-token TOKEN
--backend auto|cpu|cuda|webgpu
--install-service
```

## Variables d'environnement reconnues

- `MANIFEED_API_URL`
- `MANIFEED_EMBEDDING_ENROLLMENT_TOKEN`
- `MANIFEED_EMBEDDING_HF_TOKEN`
- `HF_TOKEN`
- `MANIFEED_EMBEDDING_EXECUTION_BACKEND`

## Notes

- le script est pense pour distribuer le worker d'embeddings, pas le worker RSS ;
- la fermeture de l'application desktop arrete le worker lance comme processus enfant ;
- l'UI desktop lit le status file local et ne scrute pas les logs pour construire son etat.
