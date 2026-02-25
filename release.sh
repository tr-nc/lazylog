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

echo "Running cargo check"
cargo check

echo "Committing version bump"
git add Cargo.toml
git commit -m "bump version to $VERSION"

echo "Creating tag v$VERSION"
git tag "v$VERSION"

echo "Pushing to remote tr-nc"
git push tr-nc
git push tr-nc "v$VERSION"

echo "Release v$VERSION created and pushed!"
