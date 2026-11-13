#!/usr/bin/env bash
# Build the WebAssembly engine and its JS bindings into web/pkg.
# Requires: rustup target add wasm32-unknown-unknown; cargo install wasm-bindgen-cli --version 0.2.100
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown -p gcgc-wasm
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/gcgc_wasm.wasm
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int -o web/pkg/gcgc_wasm_bg.wasm web/pkg/gcgc_wasm_bg.wasm
fi
ls -l web/pkg/gcgc_wasm_bg.wasm

# ./build.sh --dist : assemble a self-contained static site in dist/ (includes the example logs).
if [ "${1:-}" = "--dist" ]; then
  rm -rf dist && mkdir -p dist/datasets
  cp -R web/index.html web/app.js web/worker.js web/render.js web/style.css web/vendor web/pkg dist/
  for f in SerialGC.log ParallelGC.log G1GC.log ShenandoahGC.log ZGC.log.0 ZGC.log.1 GenZGC.log GraalVM.log example_external_data.csv; do
    cp "../datasets/$f" dist/datasets/
  done
  cp ../LICENSE dist/LICENSE.txt
  rm -f dist/pkg/*.d.ts
  # Apache (e.g. GoDaddy shared hosting): correct wasm MIME type, text for logs, compression.
  cat > dist/.htaccess <<'HTACCESS'
AddType application/wasm .wasm
AddType text/javascript .js
AddType text/plain .log .0 .1
<IfModule mod_deflate.c>
  AddOutputFilterByType DEFLATE text/html text/css text/javascript application/javascript application/wasm text/plain text/csv
</IfModule>
<IfModule mod_expires.c>
  ExpiresActive On
  ExpiresByType application/wasm "access plus 1 day"
  ExpiresByType text/javascript "access plus 1 day"
</IfModule>
HTACCESS
  # IIS (Windows/Plesk hosting) ignores .htaccess and refuses unknown file types without this.
  cat > dist/web.config <<'WEBCONFIG'
<?xml version="1.0" encoding="UTF-8"?>
<configuration>
  <system.webServer>
    <staticContent>
      <remove fileExtension=".wasm" />
      <mimeMap fileExtension=".wasm" mimeType="application/wasm" />
      <remove fileExtension=".log" />
      <mimeMap fileExtension=".log" mimeType="text/plain" />
      <remove fileExtension=".0" />
      <mimeMap fileExtension=".0" mimeType="text/plain" />
      <remove fileExtension=".1" />
      <mimeMap fileExtension=".1" mimeType="text/plain" />
    </staticContent>
    <urlCompression doStaticCompression="true" />
  </system.webServer>
</configuration>
WEBCONFIG
  rm -f gcgc-web.zip && (cd dist && zip -qr ../gcgc-web.zip .)
  du -sh dist gcgc-web.zip
fi
