# Installeur Linux du worker Embedding

Ce bundle installe le worker d'embeddings et l'application desktop partagee `Manifeed Workers`.

## Ce que contient le bundle

- `worker-source-embedding`
- `manifeed-workers`
- `install.sh`
- `manifeed-workers.svg`

## Experience d'installation

L'installation standard ne demande que :

- la cle API worker

Tout le reste part en configuration locale persistante. L'utilisateur n'a plus besoin de re-exporter
des variables d'environnement a chaque lancement.

Le script :

1. detecte le runtime ONNX adapte a la machine ;
2. copie le binaire embedding dans `~/.local/share/manifeed/embedding/` ;
3. copie l'app desktop partagee dans `~/.local/share/manifeed/desktop/manifeed-workers` ;
4. initialise `~/.config/manifeed/workers.json` via `worker-source-embedding install` ;
5. cree des lanceurs stables dans `~/.local/bin/` ;
6. peut installer un service `systemd --user`.

## Construction du bundle

Depuis la racine du repo `workers` :

```bash
./installers/linux/worker-source-embedding/build-bundle.sh
```

## Installation

### Mode interactif

```bash
./installers/linux/worker-source-embedding/install.sh --cli
```

### Mode non interactif

```bash
./installers/linux/worker-source-embedding/install.sh --non-interactive \
  --api-key mfk_live_xxxxx
```

### Installation avec service utilisateur

```bash
./installers/linux/worker-source-embedding/install.sh --install-service
```

## Lanceurs installes

- `~/.local/bin/manifeed-worker-source-embedding`
- `~/.local/bin/manifeed-workers`

## Commandes utiles apres installation

```bash
manifeed-worker-source-embedding run
manifeed-worker-source-embedding config show
manifeed-worker-source-embedding config set api-key mfk_live_xxxxx
manifeed-worker-source-embedding config set acceleration gpu
manifeed-worker-source-embedding doctor
manifeed-worker-source-embedding probe
manifeed-workers
```

## Options principales

```text
--gui
--cli
--non-interactive
--binary PATH
--desktop-binary PATH
--api-key TOKEN
--install-service
```

## Notes

- le modele est fixe a `Xenova/multilingual-e5-large` ;
- l'app desktop partagee ouvre maintenant une page `Embedding` dediee ;
- le mode `Manuel` lance le worker a la demande depuis l'app ou le CLI ;
- le mode `Service utilisateur` installe un service `systemd --user` pour laisser tourner le worker en continu ;
- l'app desktop permet de modifier `api_key`, `inference_batch_size`, l'acceleration `auto/cpu/gpu`
  et le mode de lancement sans repasser par des variables d'environnement ;
- les variables d'environnement historiques restent des overrides experts, mais ne sont plus
  le mode nominal de fonctionnement.
