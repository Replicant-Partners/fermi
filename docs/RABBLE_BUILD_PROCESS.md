# Rabble Flutter Build & Deploy Process

> Reference doc for rebuilding and deploying the Rabble Flutter web app.

## Architecture

- **Flutter source**: `/home/ilabra/rabble/` (separate git repo: `Replicant-Partners/rabble`)
- **Compiled web output**: `/home/ilabra/fermi/rabble-web/` (committed into the fermi repo)
- **Served from**: `static/rabble/` inside the Docker container (copied by `Dockerfile`)
- **Domain**: `rabble.world` — the Axum backend detects the host header and serves the Flutter SPA

## Prerequisites

- Flutter SDK: `/home/ilabra/.flutter-sdk/bin/flutter` (v3.27.4, Dart 3.6.2)
- The `rabble` repo checked out at `/home/ilabra/rabble/`
- The `fermi` repo checked out at `/home/ilabra/fermi/`

## Build Steps

```bash
# 1. Go to the Flutter source repo
cd /home/ilabra/rabble

# 2. Get dependencies (if pubspec.yaml changed)
/home/ilabra/.flutter-sdk/bin/flutter pub get

# 3. Build for web (release)
/home/ilabra/.flutter-sdk/bin/flutter build web --release

# 4. Copy build output into the fermi repo
rm -rf /home/ilabra/fermi/rabble-web/*
cp -r /home/ilabra/rabble/build/web/* /home/ilabra/fermi/rabble-web/

# 5. Commit the Flutter source fix (if any)
cd /home/ilabra/rabble
git add -A && git commit -m "describe your change" && git push

# 6. Commit the rebuilt web output in fermi and push (triggers Railway deploy)
cd /home/ilabra/fermi
git add rabble-web/
git commit -m "Rebuild Flutter web: describe what changed"
git push
```

## How Deployment Works

1. `git push` to `fermi` main branch triggers Railway auto-deploy.
2. The `Dockerfile` copies `rabble-web/` → `static/rabble/` in the container.
3. The Axum server (`api_server.rs`) checks `Host` header:
   - `rabble.world` → serves `static/rabble/index.html` (Flutter SPA)
   - Everything else → serves the Agent Bestiary UI
4. SPA routing: any path without a file extension on `rabble.world` returns `index.html`; paths with extensions (`.js`, `.wasm`, `.png`, etc.) are served directly from `static/rabble/`.

## Common Build Errors and Fixes

### Missing `dart:math` import
If you use `cos()`, `sin()`, `sqrt()`, `asin()`, `pi`, etc. in a Dart file, you need:
```dart
import 'dart:math';
```

### `LocationService` getters
The correct API for getting the user's current position:
```dart
final locService = context.read<LocationService>();
double? lat = locService.lastPosition?.latitude;
double? lng = locService.lastPosition?.longitude;
```
There are **no** `currentLat` / `currentLng` getters. Always go through `lastPosition`.

### Flutter version mismatch
The project is pinned to Flutter 3.27.4 (Dart 3.6.2). If APIs break, check:
```bash
/home/ilabra/.flutter-sdk/bin/flutter --version
```

## Key Files

| File | Purpose |
|------|---------|
| `/home/ilabra/rabble/pubspec.yaml` | Flutter dependencies |
| `/home/ilabra/rabble/lib/main.dart` | App entry point |
| `/home/ilabra/rabble/lib/services/api_client.dart` | Backend API integration |
| `/home/ilabra/rabble/lib/services/auth_service.dart` | OAuth / session management |
| `/home/ilabra/rabble/lib/services/location_service.dart` | GPS / geolocation |
| `/home/ilabra/rabble/lib/theme/rabble_theme.dart` | Colour palette and design tokens |
| `/home/ilabra/fermi/rabble-web/` | Compiled web build (committed) |
| `/home/ilabra/fermi/Dockerfile` | Copies `rabble-web/` → `static/rabble/` |
| `/home/ilabra/fermi/src/api_server.rs` | Host-aware SPA serving logic |