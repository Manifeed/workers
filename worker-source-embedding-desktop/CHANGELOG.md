# Changelog

## 0.1.1 - 2026-04-12

- bloque les actions `Update` et `Uninstall` tant qu'un worker tourne, avec revalidation cote runtime ;
- conserve le dernier status valide si `status.json` est corrompu et expose un warning persistant ;
- rend la desinstallation transactionnelle avec rollback du bundle, de la config et du service utilisateur ;
- remplace le telechargement en memoire par un flux disque + hash SHA-256 strict ;
- durcit la verification des processus externes sur Linux et macOS avant tout `stop` ;
- coalesce les `RefreshTick` pour eviter l'accumulation pendant les operations longues ;
- finalise le nettoyage du crate avec suppression du code mort et decoupage des modules longs.
