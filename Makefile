.PHONY: run build install
run:
	cargo run

build:
	cargo build --release

install: build
	mkdir -p $(HOME)/bin
	cp target/release/assetc $(HOME)/bin/assetc
