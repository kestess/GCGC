#!/usr/bin/env bash
# Build, then serve the repository root so the app can also load the example logs in datasets/.
# Open http://localhost:${PORT:-8765}/gcgc-rs/web/  (add ?examples to load the examples on start)
set -euo pipefail
cd "$(dirname "$0")"
./build.sh
PORT="${PORT:-8765}"
echo "Serving on http://localhost:$PORT/gcgc-rs/web/"
cd .. && exec python3 -m http.server "$PORT" --bind 127.0.0.1
