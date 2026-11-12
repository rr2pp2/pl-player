#!/usr/bin/env bash
# テスト用の音声ファイルを ffmpeg で作る（手元で完結。通信しない）。
# 使い方：bash scripts/make-test-audio.sh
# 出力先：src-tauri/tests/fixtures/
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=src-tauri/tests/fixtures
mkdir -p "$OUT"
SRC="sine=frequency=440:sample_rate=44100:duration=3"
COMMON=(-hide_banner -loglevel error -y)
OUTOPT=(-fflags +bitexact -map_metadata -1)
ffmpeg "${COMMON[@]}" -f lavfi -i "$SRC" "${OUTOPT[@]}" -ac 2 -c:a libmp3lame -b:a 192k -metadata title="Sine MP3" -metadata artist="Test Artist" -metadata album="Test Album" -metadata genre="Test" -metadata track=1 "$OUT/sine.mp3"
ffmpeg "${COMMON[@]}" -f lavfi -i "$SRC" "${OUTOPT[@]}" -ac 2 -c:a pcm_s16le "$OUT/sine.wav"
ffmpeg "${COMMON[@]}" -f lavfi -i "$SRC" "${OUTOPT[@]}" -ac 2 -c:a flac -metadata title="Sine FLAC" -metadata artist="Test Artist" "$OUT/sine.flac"
ffmpeg "${COMMON[@]}" -f lavfi -i "$SRC" "${OUTOPT[@]}" -ac 2 -c:a aac -b:a 160k -metadata title="Sine M4A" "$OUT/sine_aaclc.m4a"
ffmpeg "${COMMON[@]}" -f lavfi -i "$SRC" "${OUTOPT[@]}" -ac 2 -c:a alac "$OUT/sine_alac.m4a"
# HE-AAC（v3.1.2）：Windows の Media Foundation の AAC エンコーダ（aac_mf）で作る。-profile:a 4 が HE-AAC v1（SBR、明示的な合図）。
# ffmpeg の aac エンコーダは HE-AAC を作れない。HE-AAC v2（PS）は aac_mf でも v1 と同じものになる
ffmpeg "${COMMON[@]}" -f lavfi -i "$SRC" "${OUTOPT[@]}" -ac 2 -c:a aac_mf -profile:a 4 -b:a 48k -metadata title="Sine HE-AAC" "$OUT/sine_heaac.m4a"
ffmpeg "${COMMON[@]}" -f lavfi -i "$SRC" "${OUTOPT[@]}" -ac 2 -c:a aac -b:a 160k -f adts "$OUT/sine.aac"
ffmpeg "${COMMON[@]}" -f lavfi -i "$SRC" "${OUTOPT[@]}" -ac 2 -c:a libvorbis -q:a 4 -metadata title="Sine OGG" "$OUT/sine.ogg"
ffmpeg "${COMMON[@]}" -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=3" "${OUTOPT[@]}" -ac 2 -c:a libopus -b:a 96k -metadata title="Sine OPUS" "$OUT/sine.opus"
ffmpeg "${COMMON[@]}" -f lavfi -i "sine=frequency=440:sample_rate=22050:duration=3" "${OUTOPT[@]}" -ac 1 -c:a pcm_s16le "$OUT/mono_22k.wav"
# 非対応形式の例（音声ではないファイルに音声の拡張子）
printf 'this is not audio' > "$OUT/broken.mp3"
ls -la "$OUT"
# 背景とジャケットの画像
python scripts/make-test-images.py
