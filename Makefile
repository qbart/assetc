.PHONY: run build release install demo publish publish-colors publish-sdk publish-check publish-check-colors publish-check-sdk

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

# The published crates: assetc-colors, then the SDK; see docs/features/colors.md#publishing.
publish: publish-colors publish-sdk

publish-colors:
	cargo publish -p assetc-colors

publish-sdk:
	cargo publish -p assetc

publish-check: publish-check-colors publish-check-sdk

publish-check-colors:
	cargo publish -p assetc-colors --dry-run

publish-check-sdk:
	cargo publish -p assetc --dry-run
