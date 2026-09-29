POWERSHELL := powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass

.DEFAULT_GOAL := help

.PHONY: help doctor setup install dev build build-web installer test check fmt clean distclean release

help:
	@$(POWERSHELL) -Command "Write-Host 'FujiView targets:'; Write-Host '  make setup                 Install/check Windows prerequisites and npm packages'; Write-Host '  make install               Setup, build, replace any old version, and install locally'; Write-Host '  make doctor                Check prerequisites without installing'; Write-Host '  make dev                   Start the Tauri development app'; Write-Host '  make build                 Build the Windows NSIS installer'; Write-Host '  make build-web             Build only the web frontend'; Write-Host '  make test                  Run frontend and Rust tests'; Write-Host '  make check                 Run all non-mutating validation'; Write-Host '  make fmt                   Format Rust sources'; Write-Host '  make clean                 Remove generated build outputs'; Write-Host '  make distclean             Also remove node_modules'; Write-Host '  make release VERSION=x.y.z Test, tag, push and publish a release'"

doctor:
	$(POWERSHELL) -File scripts/bootstrap.ps1 -CheckOnly

setup:
	$(POWERSHELL) -File scripts/bootstrap.ps1 -InstallMissing

install: build
	$(POWERSHELL) -File scripts/install.ps1

dev: setup
	npm run tauri dev

build installer: setup
	npm run tauri build

build-web: setup
	npm run build

test: setup
	npm test
	cargo test --manifest-path src-tauri/Cargo.toml

check: setup
	npm run release:check
	npm test
	npm run build
	cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
	cargo test --manifest-path src-tauri/Cargo.toml

fmt:
	cargo fmt --manifest-path src-tauri/Cargo.toml

clean:
	$(POWERSHELL) -File scripts/clean.ps1

distclean:
	$(POWERSHELL) -File scripts/clean.ps1 -IncludeDependencies

release:
ifndef VERSION
	$(error VERSION is required. Example: make release VERSION=0.2.0)
endif
	$(POWERSHELL) -File scripts/release.ps1 -Version "$(VERSION)"
