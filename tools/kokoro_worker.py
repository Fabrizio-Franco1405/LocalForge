#!/usr/bin/env python3
"""Minimal Kokoro ONNX worker for LocalForge.

Reads the text from stdin, writes a 24 kHz mono PCM16 WAV to --out and prints
one JSON line. Uses kokoro-onnx (ONNX Runtime, CPU) -- no PyTorch, no CUDA.
"""

from __future__ import annotations

import argparse
import json
import sys
import wave
from pathlib import Path

import numpy as np
from kokoro_onnx import Kokoro

MODEL_CANDIDATES = (
    "kokoro-v1.0.int8.onnx",
    "kokoro-v1.0.fp16.onnx",
    "kokoro-v1.0.onnx",
)

LANG_BY_PREFIX = {
    "a": "en-us",
    "b": "en-gb",
    "e": "es",
    "f": "fr-fr",
    "h": "hi",
    "i": "it",
    "j": "ja",
    "p": "pt-br",
    "z": "zh",
}


def pick_model(model_dir: Path) -> Path:
    for name in MODEL_CANDIDATES:
        candidate = model_dir / name
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(
        f"no encuentro ninguno de {MODEL_CANDIDATES} en {model_dir}"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--voice", default="ef_dora")
    parser.add_argument("--speed", type=float, default=1.0)
    parser.add_argument("--lang", default="")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    text = sys.stdin.read().strip()
    if not text:
        print("texto vacío", file=sys.stderr)
        return 2

    model_dir = Path(args.model_dir).expanduser()
    model = pick_model(model_dir)
    voices = model_dir / "voices-v1.0.bin"
    if not voices.is_file():
        raise FileNotFoundError(f"falta {voices}")

    lang = args.lang or LANG_BY_PREFIX.get(args.voice[:1], "en-us")
    kokoro = Kokoro(str(model), str(voices))
    samples, sample_rate = kokoro.create(
        text, voice=args.voice, speed=max(0.5, min(2.0, args.speed)), lang=lang
    )

    pcm = (np.clip(np.asarray(samples, dtype=np.float32), -1.0, 1.0) * 32767).astype(
        np.int16
    )
    out = Path(args.out).expanduser()
    with wave.open(str(out), "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(int(sample_rate))
        handle.writeframes(pcm.tobytes())

    print(
        json.dumps(
            {
                "ok": True,
                "samples": int(pcm.size),
                "sample_rate": int(sample_rate),
                "voice": args.voice,
                "lang": lang,
            }
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # noqa: BLE001 - surface a readable message
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
