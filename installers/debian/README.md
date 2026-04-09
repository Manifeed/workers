# Packaging Debian / Ubuntu

Ce dossier construit un seul paquet `.deb` Linux pour Debian, Ubuntu et distributions derivees.

## Paquet produit

- `manifeed-workers-desktop`

Le paquet installe uniquement l'application desktop Linux. Les workers RSS et Embedding ne sont
plus distribues comme paquets systeme: ils sont telecharges et geres directement par l'app
desktop dans le home utilisateur.

## Mode de distribution

- pas de repo apt public en v1 ;
- distribution directe de fichiers `.deb` ;
- installation cible avec `apt install ./manifeed-workers-desktop_<version>_<arch>.deb`.

## Build

Depuis `workers/installers/debian` :

```bash
sudo apt install debhelper cargo rustc pkg-config curl ca-certificates \
  libasound2-dev libfontconfig1-dev libfreetype6-dev libgl1-mesa-dev \
  libudev-dev libwayland-dev libx11-dev libxcb1-dev libxkbcommon-dev
./build-debs.sh
```

Les artefacts sont copies dans `workers/dist/debian/`.

## Publication locale

Pour publier uniquement la famille desktop dans le storage backend et mettre a jour le catalogue :

```bash
../release-workers.sh --family desktop
```

Le script publie dans `../../backend/var/worker-releases/desktop/` et met a jour
`../../backend/var/worker-releases/catalog.json`.

La version de release Linux peut etre decouplee par architecture via
`artifact_version_linux_x86_64` et `artifact_version_linux_aarch64` dans
`worker-source-embedding-desktop/Cargo.toml`.

## Installation locale

```bash
sudo apt install ./manifeed-workers-desktop_0.1.0-1_amd64.deb
manifeed-workers
```

Depuis l'application desktop :

- saisir `API URL` et `API key` pour chaque worker ;
- installer ou mettre a jour `RSS` et `Embedding` independamment ;
- les telechargements de bundles passent par le backend avec Bearer worker ;
- activer le mode service utilisateur si souhaite.

## Layout installe

- binaire desktop : `/usr/lib/manifeed/desktop/manifeed-workers`
- wrapper CLI : `/usr/bin/manifeed-workers`
- app desktop : `/usr/share/applications/manifeed-workers.desktop`
- icone : `/usr/share/icons/hicolor/scalable/apps/manifeed-workers.svg`

## Notes

- aucun `postinst` ne demande de cle API ;
- aucune suppression de `~/.config/manifeed/workers.json` ;
- les workers ne sont plus installes dans `/usr`, uniquement dans les repertoires utilisateurs
  geres par l'application desktop.
