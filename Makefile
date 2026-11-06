.PHONY: run build release install demo publish publish-check

ASSETS ?= tmp/build/assetc.pak

run:
	cargo run

build:
	cargo build

release:
	cargo build --release

install: release
	mkdir -p $(HOME)/bin
	cp target/release/assetc $(HOME)/bin/assetc

demo:
	cargo run --release -- pack
	cargo run --release -p assetc-demo -- $(ASSETS)

# The SDK is the only published crate; see docs/features/sdk.md.
publish-check:
	cargo publish -p assetc --dry-run

publish:
	cargo publish -p assetc
