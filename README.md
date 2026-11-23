# zcstream-app — Streaming Vidéo RUDP

Interface graphique native accélérée GPU (`Rune` + `wgpu`) dédiée au **streaming vidéo ultra-basse latence** en UDP direct ou en pair-à-pair via ZPSync NAT Traversal (`zcstream`, `zcvid`, `zcrudp`, `zpsync`).

## Téléchargement direct (Releases GitHub)

Téléchargez l'archive précompilée pour votre plateforme depuis **[GitHub Releases](https://github.com/i-Charlys/zcstream-app/releases)** (`windows-x64`, `windows-arm64`, `linux-x64`, `linux-arm64`), décompressez-la et lancez directement `zcstream-app`. Aucune compilation ni configuration manuelle requise.

## Compilation depuis les sources (1 commande)

Vous n'avez **pas** besoin de cloner les 5 dépôts manuellement ni de modifier des chemins d'accès :

### Windows (PowerShell)
```powershell
git clone https://github.com/i-Charlys/zcstream-app.git
cd zcstream-app
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

### Linux / macOS
```bash
git clone https://github.com/i-Charlys/zcstream-app.git
cd zcstream-app
chmod +x ./install.sh
./install.sh
```

L'installeur récupère automatiquement les dépôts compagnons (`zcrudp`, `zcvid`, `zpsync`, `zcstream`), compile les binaires en `Release` et assemble un bundle autonome dans `../dist/`.

## Surcharges d'environnement (optionnelles)

`zcstream-app` découvre automatiquement les exécutables (`zcrudp_stream`, `zpsync_peer`) à côté de l'exécutable, dans `../dist/`, dans `../zcstream/build/{,Release,Debug}`, ou via le `PATH` :
- `ZCSTREAM_BIN_DIR` : dossier contenant les binaires `zcstream`.
- `ZCSTREAM_ROOT` : dossier racine contenant les dépôts compagnons.

## Licence & Citations

Ce projet et l'ensemble des dépôts de la suite (`zcstream-app`, `zcstream`, `zcvid`, `zcrudp`, `zpsync`) sont distribués sous licence **MIT** — voir [LICENSE](LICENSE).

Les attributions aux bibliothèques tierces (`Rune`, `wgpu`, `winit`, FFmpeg `nv-codec-headers`, `Pyrowave`) ainsi que les références scientifiques et standards (RFC 8439, RFC 7748, RFC 8298, Cauchy FEC, H.264/HEVC, Jayant ADM, STUN/DHT) sont détaillées dans [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
