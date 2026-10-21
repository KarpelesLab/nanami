#!/bin/sh
# Objective proxies for listening tests: Whisper WER (small + medium) and
# UTMOS (predicted MOS) over 40 Harvard sentences.
#   usage: tools/eval/bench.sh [voice] [verbose]
# Needs a Python env with faster-whisper, scipy, torch (CPU is fine for UTMOS);
# set NANAMI_EVAL_PY to its interpreter and NANAMI_EVAL_LIBS to extra
# LD_LIBRARY_PATH entries for CUDA (cublas/cudnn) if needed.
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
PY=${NANAMI_EVAL_PY:-python3}
OUT=${NANAMI_EVAL_OUT:-/tmp/nanami-eval}
mkdir -p "$OUT/hub"
export LD_LIBRARY_PATH="${NANAMI_EVAL_LIBS:+$NANAMI_EVAL_LIBS:}$LD_LIBRARY_PATH"
cargo build --release --manifest-path "$ROOT/Cargo.toml" -q
BIN=$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --format-version 1 --no-deps | "$PY" -c "import json,sys; print(json.load(sys.stdin)['target_directory'])")/release/nanami
V=${1:-nanami}
i=0; files=""
while read -r line; do
  i=$((i+1)); "$BIN" say -v "$V" -o "$OUT/s$i.wav" "$line" >/dev/null 2>&1; files="$files $OUT/s$i.wav"
done < "$HERE/harvard40.txt"
for M in small medium; do
  "$PY" -I "$HERE/asr.py" $M $files 2>/dev/null | grep -v -i warn > "$OUT/out_$M.txt"
  printf "[%s] " $M; "$PY" -I "$HERE/wer.py" "$HERE/harvard40.txt" "$OUT/out_$M.txt" $2 | if [ -n "$2" ]; then cat; else tail -1; fi
done
(cd "$OUT" && "$PY" -I "$HERE/mos.py" "$OUT/hub" $files 2>/dev/null | tail -1)
