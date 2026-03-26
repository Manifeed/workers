# Installeur Linux du worker RSS

Ce bundle installe le worker RSS et l'application desktop partagee `Manifeed Workers`.

## Ce que contient le bundle

- `worker-rss`
- `manifeed-workers`
- `install.sh`
- `manifeed-workers.svg`

## Experience d'installation

L'installation standard ne demande que :

- la cle API worker

Le script :

1. copie le binaire RSS dans `~/.local/share/manifeed/rss/` ;
2. copie l'app desktop partagee dans `~/.local/share/manifeed/desktop/manifeed-workers` ;
3. initialise `~/.config/manifeed/workers.json` via `worker-rss install` ;
4. cree des lanceurs stables dans `~/.local/bin/` ;
5. peut installer un service `systemd --user`.

## Construction du bundle

Depuis la racine du repo `workers` :

```bash
./installers/linux/worker-rss/build-bundle.sh
```

## Installation

### Mode interactif

```bash
./installers/linux/worker-rss/install.sh --cli
```

### Mode non interactif

```bash
./installers/linux/worker-rss/install.sh --non-interactive \
  --api-key mfk_live_xxxxx
```

### Installation avec service utilisateur

```bash
./installers/linux/worker-rss/install.sh --install-service
```

## Lanceurs installes

- `~/.local/bin/manifeed-worker-rss`
- `~/.local/bin/manifeed-workers`

## Commandes utiles apres installation

```bash
manifeed-worker-rss run
manifeed-worker-rss config show
manifeed-worker-rss config set api-key mfk_live_xxxxx
manifeed-worker-rss doctor
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

- la configuration persistante locale remplace l'installation basee sur les env vars ;
- l'app desktop partagee ouvre maintenant une page `Scraping` dediee ;
- le mode `Manuel` lance le worker a la demande depuis l'app ou le CLI ;
- le mode `Service utilisateur` installe un service `systemd --user` pour laisser tourner le worker en continu ;
- l'app desktop permet ensuite de modifier `api_key`, le mode de lancement et `max_in_flight_requests` ;
- les variables d'environnement historiques restent des overrides experts uniquement.
