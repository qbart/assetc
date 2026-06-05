.PHONY: run
run: build
	cd build && ./assetc || cd ..

.PHONY: build
build:
	@cmake --build build/debug

.PHONY: install
install: build
	cp build/assetc ~/bin/assetc

.PHONY: clean
clean:
	@rm -rf build/

.PHONY: setup
setup:
	sudo apt install libcli11-dev libfmt-dev

.PHONY: init
init:
	cmake --preset debug .

.PHONY: init.release
init.release:
	cmake --preset release .

.PHONY: release
release:
	@cmake --build build/release
