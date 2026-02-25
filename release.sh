#!/bin/bash

set -e

if [ -z "$1" ]; then
	echo "Usage: $0 <version>"
	echo "Example: $0 0.5.2"
	exit 1
fi

VERSION="$1"

echo "Updating version to $VERSION in Cargo.toml"
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

echo "Running cargo fmt"
cargo fmt

echo "Running cargo clippy"
cargo clippy --release -- -D warnings

echo "Committing version bump"
git add -A
git commit -m "bump version to $VERSION"

echo "Creating tag v$VERSION"
git tag "v$VERSION"

echo "Pushing to remote origin"
git push origin
git push origin "v$VERSION"

echo "Release v$VERSION created and pushed!"
