#!/usr/bin/env bash
# Concern: materializes every corpus.toml repo at its pinned rev under target/corpus/ | Non-concern: choosing the repos, or what the tests assert over them | IO: (corpus.toml) -> target/corpus/<name>/
# Idempotent: a repo already at its pinned rev is left alone, so re-running costs nothing.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$root/corpus.toml"
dest_root="${CORPUS_DIR:-$root/target/corpus}"

# Field-order-independent read of corpus.toml; no TOML parser in a shell script.
parse() {
	awk '
		function flush() { if (name != "") print name "\t" url "\t" rev; name = ""; url = ""; rev = "" }
		/^\[\[repo\]\]/ { flush(); next }
		/^[a-z_]+[ \t]*=/ {
			key = $1
			line = $0
			sub(/^[^=]*=[ \t]*/, "", line)
			sub(/[ \t]*#.*$/, "", line)
			gsub(/^"|"[ \t]*$/, "", line)
			if (key == "name") name = line
			if (key == "url")  url = line
			if (key == "rev")  rev = line
		}
		END { flush() }
	' "$manifest"
}

parse | while IFS=$'\t' read -r name url rev; do
	dir="$dest_root/$name"
	if [ "$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)" = "$rev" ]; then
		echo "corpus: $name already at $rev"
		continue
	fi
	echo "corpus: fetching $name $rev"
	rm -rf "$dir"
	mkdir -p "$dir"
	git -C "$dir" init --quiet
	git -C "$dir" remote add origin "$url"
	git -C "$dir" fetch --quiet --depth 1 origin "$rev"
	git -C "$dir" checkout --quiet FETCH_HEAD
done

echo "corpus: ready at $dest_root"
