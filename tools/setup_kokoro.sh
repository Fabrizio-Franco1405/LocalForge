#!/usr/bin/env bash
# Crea el venv aislado y descarga los modelos de Kokoro (ONNX, CPU).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
venv="$root/.venv-kokoro"
models="$root/assets/models/kokoro"
base="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"

python3 -m venv "$venv"
"$venv/bin/pip" install --upgrade pip
"$venv/bin/pip" install kokoro-onnx

mkdir -p "$models"
for file in kokoro-v1.0.int8.onnx voices-v1.0.bin; do
  if [[ ! -f "$models/$file" ]]; then
    echo "descargando $file…"
    curl -L --fail -o "$models/$file" "$base/$file"
  fi
done

echo
echo "Listo. Configura en Ajustes → Motor Audio:"
echo "  Python:        $venv/bin/python"
echo "  Worker:        $root/tools/kokoro_worker.py"
echo "  Carpeta modelo:$models"
echo "  Voz:           ef_dora (español)"
