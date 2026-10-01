#!/bin/sh
# Reproduce keel's historical study population with the public phase-1 exporter.
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
output="$project/docs/studies/template-respelled"
mkdir -p /var/tmp/keel-87bf1
export TMPDIR=/var/tmp/keel-87bf1 CARGO_BUILD_JOBS=6
if [ "$(awk '/MemAvailable:/ {print $2}' /proc/meminfo)" -lt 4194304 ]; then
    sleep 60
    export CARGO_BUILD_JOBS=4
fi
scratch=$(mktemp -d "$TMPDIR/study.XXXXXX")
exporter_sha=a9e3ade
source_sha=911cca1748edaa8bb0056c50283ad1e41731060b
mkdir -p "$scratch/exporter" "$scratch/keel"
git -C "$project" archive "$exporter_sha" > "$scratch/exporter.tar"
tar -xf "$scratch/exporter.tar" -C "$scratch/exporter"
(cd "$scratch/exporter" && CARGO_TARGET_DIR="$scratch/target" cargo build --workspace --locked > "$scratch/exporter-build.log" 2>&1)
binary="$scratch/target/debug/keel"
git -C "$project" archive "$source_sha" > "$scratch/keel.tar"
tar -xf "$scratch/keel.tar" -C "$scratch/keel"
export GIT_AUTHOR_DATE='2000-01-01T00:00:00Z' GIT_COMMITTER_DATE='2000-01-01T00:00:00Z'
fixture_git() {
    git -c core.hooksPath=/dev/null -c user.name='keel study' \
        -c user.email='study@keel.dev' -c commit.gpgsign=false "$@"
}
dir="$scratch/keel"
fixture_git -C "$dir" init -q
fixture_git -C "$dir" add -f .
fixture_git -C "$dir" commit -qm 'pinned source tree'
empty_tree=$(fixture_git -C "$dir" mktree < /dev/null)
empty_base=$(printf '%s\n' 'orphan empty study base' | fixture_git -C "$dir" commit-tree "$empty_tree")
fixture_git -C "$dir" update-ref refs/heads/empty-base "$empty_base"
(cd "$dir" && "$binary" map --json > "$scratch/keel-map.json" 2> "$scratch/keel-map.log")
(cd "$dir" && "$binary" review --json --base empty-base > "$scratch/keel-review.json" 2> "$scratch/keel-review.log")
uv run --no-project python - "$scratch" "$output" "$source_sha" <<'PY'
import copy
import json
import random
import sys
from pathlib import Path

scratch, output = map(Path, sys.argv[1:3])
sha = sys.argv[3]
full = json.loads((scratch / 'keel-review.json').read_text())['template_study']
for minimum in (16, 24):
    study = copy.deepcopy(full)
    study.update(tree='keel', source_commit=sha, min_segment_chars=minimum)
    for home in study['homes']:
        home['segments'] = [s for s in home['segments'] if len(s) >= minimum]
    study['kept_segments'] = len({s for h in study['homes'] for s in h['segments']})
    study['occurrences'] = [o for o in study['occurrences'] if len(o['segment']) >= minimum]
    for occurrence in study['occurrences']:
        for home in occurrence['homes']:
            home['segments'] = [s for s in home['segments'] if len(s) >= minimum]
    study['occurrence_count'] = len(study['occurrences'])
    stem = f'keel-{minimum}'
    (output / f'{stem}.json').write_text(json.dumps(study, ensure_ascii=False, separators=(',', ':')) + '\n')
    sample = random.Random(87).sample(list(enumerate(study['occurrences'])), min(50, study['occurrence_count']))
    def cell(value):
        return str(value).replace('\\', '\\\\').replace('|', '\\|').replace('\n', '<br>').replace('\r', '').replace('\0', '⟨interpolation⟩')
    rows = [f'# keel, minimum {minimum} characters', '',
            f'Source `{sha}`. Seed 87; sample size {len(sample)} / {study["occurrence_count"]}. Verdicts intentionally empty.', '',
            '| Export index (0-based) | Homes | Literal location | Segment | Decoded literal | Verdict |',
            '| --- | --- | --- | --- | --- | --- |']
    for index, occurrence in sample:
        homes = ', '.join(f'{h["name"]} ({h["file"]}:{h["line"]})' for h in occurrence['homes'])
        values = [index, homes, f'{occurrence["file"]}:{occurrence["line"]}', occurrence['segment'], occurrence['literal']]
        verdict = 'FALSE — Generic count wording shared by two different reports (the checkpoint header vs the review counts line, with different fields); calling counts_line would be wrong.'
        rows.append('| ' + ' | '.join(map(cell, values)) + f' | {verdict} |')
    (output / f'{stem}-sample.md').write_text('\n'.join(rows) + '\n')
    print(f'{stem}: {study["template_functions"]} homes, {study["kept_segments"]} segments, {study["occurrence_count"]} occurrences')
print(f'Scratch evidence: {scratch}')
PY
