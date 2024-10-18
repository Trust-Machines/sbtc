# Convenience data so we can run the following and include
# sources from three directories deep.
#
# Example:
# $(subst dir, emily/cdk/lib, $(THREE_DIRS_DEEP))
# becomes
# emily/cdk/lib/*  emily/cdk/lib/*/*  emily/cdk/lib/*/*/*
#
# TODO(TBD): Find a better way to do this.
ONE_DIR_DEEP    := dir/*
TWO_DIRS_DEEP   := dir/* $(subst dir, dir/*, $(ONE_DIR_DEEP))
THREE_DIRS_DEEP := dir/* $(subst dir, dir/*, $(TWO_DIRS_DEEP))
FOUR_DIRS_DEEP  := dir/* $(subst dir, dir/*, $(THREE_DIRS_DEEP))
FIVE_DIRS_DEEP  := dir/* $(subst dir, dir/*, $(FOUR_DIRS_DEEP))

# Common Folders.
AUTOGENERATED_SOURCES := ./.generated-sources

# Blocklist Client Files
AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT := $(AUTOGENERATED_SOURCES)/blocklist-api/src/lib.rs
BLOCKLIST_OPENAPI_PATH := $(AUTOGENERATED_SOURCES)/blocklist-openapi-gen
BLOCKLIST_OPENAPI_SPEC := $(BLOCKLIST_OPENAPI_PATH)/blocklist-client-openapi.json

# Emily API Files
EMILY_OPENAPI_PATH := $(AUTOGENERATED_SOURCES)/emily/openapi
EMILY_OPENAPI_SPEC := $(EMILY_OPENAPI_PATH)/emily-openapi-spec.json
AUTOGENERATED_EMILY_CLIENT := $(AUTOGENERATED_SOURCES)/emily/client/rust/src/lib.rs
EMILY_LAMBDA_BINARY := target/lambda/emily-handler/bootstrap.zip
EMILY_CDK_TEMPLATE := emily/cdk/cdk.out/EmilyStack.template.json

# File that's updated whenever there's a new pnpm installation.
INSTALL_TARGET := pnpm-lock.yaml

# Don't use the install target here so you can rerun install without
# Makefile complaints.
export DATABASE_URL=postgres://user:password@localhost:5432/signer

install:
	pnpm install

build:
	cargo build
	pnpm --recursive build

test:
	cargo test -- --test-threads=1
	pnpm --recursive test

lint:
	cargo clippy -- -D warnings
	pnpm --recursive run lint

format:
	cargo fmt

clean:
	cargo clean
	pnpm --recursive clean

.PHONY: install build test lint format clean

# Integration tests.
# ------------------------------------------------------------------------------

temp:
	cargo test --test integration --all-features --no-fail-fast --package emily-handler -- \
		chainstate::create_and_get_chainstate_reorg_happy_path::reorg_to_tip_at_same_height \
		--test-threads=1

integration-env-up: $(INSTALL_TARGET)
	docker compose --file docker/docker-compose.test.yml up --detach

integration-test: $(INSTALL_TARGET) $(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT)
	cargo test --test integration --all-features --no-fail-fast -- --test-threads=1

integration-env-down: $(INSTALL_TARGET)
	docker compose --file docker/docker-compose.test.yml down

integration-test-full: integration-env-down integration-env-up integration-test integration-env-down

.PHONY: integration-env-up integration-test integration-env-up integration-test-full

# Emily API
# ----------------------------------------------------

# Project Names
## Cargo crates
EMILY_HANDLER_PROJECT_NAME := emily-handler
EMILY_OPENAPI_SPEC_PROJECT_NAME := emily-openapi-spec
## Node projects
EMILY_CDK_PROJECT_NAME := emily-cdk

# Emily CDK Template ---------------------------------

EMILY_CDK_SOURCE_FILES := $(wildcard $(subst dir, emily/cdk/lib, $(FIVE_DIRS_DEEP)))
EMILY_CDK_SOURCE_FILES := $(wildcard $(subst dir, emily/bin/lib, $(FIVE_DIRS_DEEP))) $(EMILY_CDK_SOURCE_FILES)

$(EMILY_CDK_TEMPLATE): $(INSTALL_TARGET) $(EMILY_CDK_SOURCE_FILES)
	AWS_STAGE=local \
	TABLES_ONLY=true \
	pnpm --filter $(EMILY_CDK_PROJECT_NAME) run synth

# Emily Handler --------------------------------------

emily-as-lambda: $(EMILY_LAMBDA_BINARY)
emily-cdk-synth: $(EMILY_CDK_TEMPLATE)
emily-openapi-spec: $(EMILY_OPENAPI_SPEC)
emily-client: $(AUTOGENERATED_EMILY_CLIENT)

.PHONY: emily-lambda emily-cdk-synth emily-openapi-spec emily-client

# Build the zipped binary for the Emily Handler that AWS Lambda can deploy.
#
# Date: 10-18-2024
# The Emily lamdba binary cannot be built on aarm64 Machines because the `cargo lambda`
# compiler cannot compile an assembly library that is used in the `stacks-common`
# crate that is a downstream dependency of the `emily-handler` crate.
#
# aarm64 machines can still create the x86_64 binary by running the following command, but
# it will not be runnable using the SAM CLI on aarm64 machines.
EMILY_HANDLER_SOURCE_FILES := $(wildcard $(subst dir, emily/handler, $(FIVE_DIRS_DEEP)))
$(EMILY_LAMBDA_BINARY): $(EMILY_HANDLER_SOURCE_FILES)
	cargo lambda build \
		--release \
		--package $(EMILY_HANDLER_PROJECT_NAME) \
		--output-format zip

# Generate the client code using the OpenAPI spec
$(AUTOGENERATED_EMILY_CLIENT): $(INSTALL_TARGET) $(EMILY_OPENAPI_SPEC)
	@echo "Building emily client from Openapi Spec"
	pnpm --prefix $(EMILY_OPENAPI_PATH) run build

# Build the OpenAPI specification.
$(EMILY_OPENAPI_SPEC): $(INSTALL_TARGET) $(EMILY_HANDLER_SOURCE_FILES)
	cargo build --package $(EMILY_OPENAPI_SPEC_PROJECT_NAME)

# Devenv
# ----------------------------------------------------

devenv-up:
	docker compose -f docker/docker-compose.yml --profile bitcoin-mempool up --detach

devenv-down:
	docker compose -f docker/docker-compose.yml --profile bitcoin-mempool down

# Blocklist Client API
# ----------------------------------------------------

# Generate the client code using the OpenAPI spec
$(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT): $(BLOCKLIST_OPENAPI_SPEC)
	pnpm --prefix $(BLOCKLIST_OPENAPI_PATH) run build

# Generate the OpenAPI spec for Blocklist Client
BLOCKLIST_OPENAPI_SPEC_SOURCE_FILES := $(filter-out $(BLOCKLIST_OPENAPI_SPEC), $(wildcard $(subst dir, $(BLOCKLIST_OPENAPI_PATH), $(THREE_DIRS_DEEP))))
$(BLOCKLIST_OPENAPI_SPEC): $(INSTALL_TARGET) $(BLOCKLIST_OPENAPI_SPEC_SOURCE_FILES)
	cargo build --package blocklist-openapi-gen

# Signer
# ----------------------------------------------------

run-signer:
	docker compose --file docker-compose.signer.yml down;
	docker compose --file docker-compose.signer.yml up postgres bitcoind --detach;
	POSTGRES_PORT="0"; \
	while [ "$$POSTGRES_PORT" -le 0 ]; do \
		sleep 1; \
		POSTGRES_PORT=$$(docker port sbtc-postgres 5432 | awk -F: '{print $$2}'); \
	done; \
	echo $$POSTGRES_PORT; \
	RUST_LOG=info SIGNER_SIGNER__DB_ENDPOINT="postgres://devenv:devenv@localhost:$$POSTGRES_PORT/signer" cargo run --bin signer -- -c ./signer/src/config/default.toml --migrate-db

.PHONY: run-signer
