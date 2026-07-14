.PHONY: install dev build lint check clean release quality tag

VERSION ?= 0.1.0

install:
	npm install

dev:
	npm run dev

build:
	npm run build
	cd src-tauri && cargo build

lint:
	cd src-tauri && cargo clippy --all-targets -- -D warnings
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

quality: lint check

tag: quality
	@git diff --quiet || (echo "ERROR: working tree is dirty, commit or stash first" && exit 1)
	@git fetch --tags origin
	@git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null && \
		(echo "ERROR: tag v$(VERSION) already exists" && exit 1) || true
	git tag -a "v$(VERSION)" -m "Release v$(VERSION)"
	git push origin "v$(VERSION)"
	@echo "Pushed tag v$(VERSION) — GitHub Actions will build the installer."
