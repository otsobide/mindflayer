.DEFAULT_GOAL := build
.PHONY: install build test lint fmt types dev e2e clean

# pnpm at the version package.json's packageManager pins. Where it is not on
# the PATH and corepack is not set up, name it some other way:
#   make web/test PNPM='npx --yes pnpm@12.15.0'
PNPM ?= pnpm

# pnpm writes this as it installs, and every install here touches it after,
# so it is newer than the manifest, the lockfile and pnpm's settings only
# when node_modules holds what they say. Every target that runs a tool waits
# on it, which installs on a fresh clone and after a change to any of them,
# and costs nothing otherwise.
INSTALLED := node_modules/.modules.yaml

$(INSTALLED): package.json pnpm-lock.yaml pnpm-workspace.yaml
	$(PNPM) install --frozen-lockfile
	@touch $@

install: ## Install exactly what the lockfile pins
	$(PNPM) install --frozen-lockfile
	@touch $(INSTALLED)

build: $(INSTALLED) ## Check the types, then build the panel into dist/, which the server embeds
	$(PNPM) run build

test: $(INSTALLED) ## Run the panel's tests (vitest)
	$(PNPM) run test

lint: $(INSTALLED) ## Lint the panel, its style included (ESLint)
	$(PNPM) run lint

fmt: $(INSTALLED) ## Fix what the linter can fix, its style above all
	$(PNPM) run fmt

types: $(INSTALLED) ## Check the panel's types (vue-tsc)
	$(PNPM) run typecheck

dev: $(INSTALLED) ## Serve the panel on 127.0.0.1:5173, /api proxied to `flayer ui --dev --port 7878`
	$(PNPM) run dev

e2e: ## End-to-end tests in a browser: there are none yet (#75)
	@echo "no end-to-end suite yet: #75 adds it, with Playwright"
	@exit 1

clean: ## Remove the built panel and the tools' caches, keeping node_modules
	rm -rf dist node_modules/.cache node_modules/.vite node_modules/.tmp
