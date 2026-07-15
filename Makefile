SHELL := cmd.exe

.PHONY: install dev build lint check clean release quality tag bump

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
	if exist dist rmdir /s /q dist
	if exist node_modules rmdir /s /q node_modules

release:
	npm run build
	cd src-tauri && cargo build --release

quality: lint check

bump:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bump-version.ps1 $(VERSION)

tag: quality bump
	git add -A
	git diff --cached --quiet || git commit -m "chore(release): v$(VERSION)"
	git fetch --tags origin
	git show-ref --verify --quiet "refs/tags/v$(VERSION)" && set EXISTS=1 || set EXISTS=0
	if "%EXISTS%"=="1" (echo ERROR: tag v$(VERSION) already exists & exit /b 1)
	git push origin HEAD
	git tag -a "v$(VERSION)" -m "Release v$(VERSION)"
	git push origin "v$(VERSION)"
	@echo Pushed tag v$(VERSION) - GitHub Actions will build the installer.
