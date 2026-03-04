.PHONY: help dev build check deploy

help:
	@echo "Available targets:"
	@echo "  make dev    - Run local Worker dev server"
	@echo "  make build  - Build Worker artifacts"
	@echo "  make check  - Run Rust compile checks"
	@echo "  make deploy - Deploy Worker to Cloudflare"

dev:
	wrangler dev

build:
	cargo install -q worker-build
	worker-build --release

check:
	cargo check --target wasm32-unknown-unknown

deploy:
	wrangler deploy
