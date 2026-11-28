.PHONY: run build release install demo publish publish-colors publish-atlas publish-noise publish-sdk publish-check publish-check-colors publish-check-atlas publish-check-noise publish-check-sdk

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

# The published crates: assetc-colors, assetc-atlas and assetc-noise (they depend on colors),
# then the SDK; see docs/features/colors.md#publishing.
publish: publish-colors publish-atlas publish-noise publish-sdk

publish-colors:
	cargo publish -p assetc-colors

publish-atlas:
	cargo publish -p assetc-atlas

publish-noise:
	cargo publish -p assetc-noise

publish-sdk:
	cargo publish -p assetc

publish-check: publish-check-colors publish-check-atlas publish-check-noise publish-check-sdk

publish-check-colors:
	cargo publish -p assetc-colors --dry-run

publish-check-atlas:
	cargo publish -p assetc-atlas --dry-run

publish-check-noise:
	cargo publish -p assetc-noise --dry-run

publish-check-sdk:
	cargo publish -p assetc --dry-run
