.PHONY: run build install demo

ASSETS ?= tmp/build/assetc.pak

run:
	cargo run

build:
	cargo build --release

install: build
	mkdir -p $(HOME)/bin
	cp target/release/assetc $(HOME)/bin/assetc

demo:
	cargo run --release -- pack
	cargo run --release -p assetc-demo -- $(ASSETS)
