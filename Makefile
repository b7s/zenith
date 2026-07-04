.PHONY: install dev build lint check clean release

install:
	npm install

dev:
	npm run dev

build:
	npm run build
	cd src-tauri && cargo build

lint:
	cd src-tauri && cargo clippy -- -D warnings
	npm run lint

check:
	cd src-tauri && cargo check
	cd src-tauri && cargo test
	npm run build

clean:
	cd src-tauri && cargo clean
	rm -rf dist node_modules

release:
	npm run build
	cd src-tauri && cargo build --release
