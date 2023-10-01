#!/bin/sh

# Use cargo install toml-cli to install toml

version=$1

CARGO_TOML_FILES="libnp/Cargo.toml np/Cargo.toml nport-server/Cargo.toml"
for file in $CARGO_TOML_FILES; do
    contents=$(toml set $file package.version $version)
    echo "$contents" > $file
done

git add ${CARGO_TOML_FILES}
git commit -m "chore: bump version to $version"
git tag -s v$version -m "release: version $version"
