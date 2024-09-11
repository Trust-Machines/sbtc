# Convenience data so we can run the following and include
# sources from three directories deep.
#
# Example:
# $(subst dir, emily/cdk/lib, $(THREE_DIRS_DEEP))
# becomes
# emily/cdk/lib/*  emily/cdk/lib/*/*  emily/cdk/lib/*/*/*
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
EMILY_DOCKER_COMPOSE := docker-compose.emily.yml

# File that's updated whenever there's a new pnpm installation.
INSTALL_TARGET := pnpm-lock.yaml

# Don't use the install target here so you can rerun install without
# Makefile complaints.
export DATABASE_URL=postgres://user:password@localhost:5432/signer

install:
	pnpm install
	touch pnpm-lock.yaml

build: $(INSTALL_TARGET) $(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT) $(AUTOGENERATED_EMILY_CLIENT)
	cargo build
	pnpm --recursive build

test: $(INSTALL_TARGET) $(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT) $(AUTOGENERATED_EMILY_CLIENT)
	cargo test -- --test-threads=1
	pnpm --recursive test

lint: $(INSTALL_TARGET) $(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT) $(AUTOGENERATED_EMILY_CLIENT)
	cargo clippy -- -D warnings
	pnpm --recursive run lint

format:
	cargo fmt

clean:
	cargo clean
	pnpm --recursive clean
	rm -rf devenv/dynamodb/data/*
	@touch package.json


.PHONY: install build test lint format clean

$(INSTALL_TARGET): $(wildcard package* */package* */*/package*)
	pnpm install
	touch pnpm-lock.yaml

# Integration tests.
# ------------------------------------------------------------------------------

integration-env-up: $(INSTALL_TARGET) $(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT)
	docker compose --file docker-compose.test.yml up --detach

integration-test: $(INSTALL_TARGET) $(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT)
	cargo test --package signer --test integration --all-features -- --test-threads=1

integration-env-down: $(INSTALL_TARGET) $(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT)
	docker compose --file docker-compose.test.yml down

integration-test-full: integration-env-up integration-test integration-env-down

.PHONY: integration-env-up integration-test integration-env-up integration-test-full

# Emily API
# ----------------------------------------------------

# Project Names
## Cargo crates
EMILY_HANDLER_PROJECT_NAME := emily-handler
EMILY_OPENAPI_SPEC_PROJECT_NAME := emily-openapi-spec
## Node projects
EMILY_CDK_PROJECT_NAME := emily-cdk

# Set container host environment variable depending on the local
# system of the host.
ifeq ($(findstring Linux, $(shell uname)), Linux)
_CONTAINER_HOST := localhost
else
_CONTAINER_HOST := host.docker.internal
endif

# Emily Integration tests.
# ------------------------------------------------------------------------------
# TODO(505): Combine these integration tests with the other integration tests.

# Runs a version of the emily integration environment with a pre-populated database. This is intended
# to be used for testing the sbtc bridge website, and is not intended to be used for running code that
# alters the API - though it could be used for that.
emily-integration-env-up-populated-database: $(EMILY_CDK_TEMPLATE) $(EMILY_DOCKER_COMPOSE) # devenv
	DYNAMODB_DB_DIR=./devenv/dynamodb/populated \
		CONTAINER_HOST=$(_CONTAINER_HOST) \
		docker compose --file $(EMILY_DOCKER_COMPOSE) up --remove-orphans # --detach

# Populates the Emily API database.
emily-populate-database:
	@echo "Populating populated database"
	cargo test --package emily-handler --test integration --features populate -- --test-threads=1 --nocapture

emily-integration-env-up: $(EMILY_DOCKER_COMPOSE) $(EMILY_CDK_TEMPLATE) $(EMILY_DOCKER_COMPOSE) devenv
	rm -rf ./devenv/dynamodb/dynamic/shared-local-instance.db
	DYNAMODB_DB_DIR=./devenv/dynamodb/dynamic \
		CONTAINER_HOST=$(_CONTAINER_HOST) \
		docker compose --file $(EMILY_DOCKER_COMPOSE) up --remove-orphans # --detach

emily-integration-test:
	cargo test --package emily-handler \
		--test integration \
		--features integration-tests -- \
		--test-threads=1 --nocapture
	cargo test --package emily-client-tests \
		--test integration \
		--all-features -- \
		--test-threads=1 --nocapture

emily-complex-integration-test:
	cargo test --package emily-handler --test integration --features integration-tests -- \
		complex \
		endpoints::chainstate::backfill_chainstate_causes_error \
		--test-threads=1 --nocapture

emily-integration-env-down:
	CONTAINER_HOST=$(_CONTAINER_HOST) docker compose --file $(EMILY_DOCKER_COMPOSE) down

emily-integration-test-full: emily-integration-env-up emily-integration-test emily-integration-env-down

emily-server-watch:
	cargo watch -d 1.5 -x 'run --bin emily-server -- --pretty-logs'

emily-integration-test-watch:
	cargo watch -d 3 -x 'test --package emily-handler --test integration --features integration-tests -- \
		--test-threads=1 \
		--nocapture'

.PHONY: emily-integration-env-up emily-integration-test emily-integration-env-up emily-integration-test-full

# Builds all dockerfiles that need to be built for the dev environment.
devenv: $(wildcard $(subst dir, devenv, $(TWO_DIRS_DEEP)))
	DYNAMODB_DB_DIR=./devenv/dynamodb/pre-populated \
		CONTAINER_HOST=$(_CONTAINER_HOST) \
		docker compose --file $(EMILY_DOCKER_COMPOSE) build
	@touch devenv

# Emily CDK Template ---------------------------------

EMILY_CDK_SOURCE_FILES := $(wildcard $(subst dir, emily/cdk/lib, $(FIVE_DIRS_DEEP)))
EMILY_CDK_SOURCE_FILES := $(wildcard $(subst dir, emily/bin/lib, $(FIVE_DIRS_DEEP))) $(EMILY_CDK_SOURCE_FILES)

$(EMILY_CDK_TEMPLATE): $(INSTALL_TARGET) $(EMILY_OPENAPI_SPEC) $(EMILY_CDK_SOURCE_FILES)
	AWS_STAGE=local \
	TABLES_ONLY=true \
	pnpm --filter $(EMILY_CDK_PROJECT_NAME) run synth

# Emily Handler --------------------------------------

EMILY_HANDLER_SOURCE_FILES := $(wildcard $(subst dir, emily/handler, $(FIVE_DIRS_DEEP)))

# Build the OpenAPI specification.
$(EMILY_OPENAPI_SPEC): $(EMILY_HANDLER_SOURCE_FILES)
	cargo build --package $(EMILY_OPENAPI_SPEC_PROJECT_NAME)

# Build the zipped binary for the Emily Handler that AWS Lambda can deploy.
ifneq ($(filter arm64 aarch64, $(shell uname -m)),)
_LAMBDA_FLAGS := --arm64
endif
$(EMILY_LAMBDA_BINARY): $(EMILY_HANDLER_SOURCE_FILES)
	cargo lambda build \
		--release \
		--package $(EMILY_HANDLER_PROJECT_NAME) \
		--output-format zip \
		$(_LAMBDA_FLAGS)

# Emily Manual Task Triggers -------------------------

emily-lambda: $(EMILY_LAMBDA_BINARY)
emily-cdk-synth: $(EMILY_CDK_TEMPLATE)
emily-openapi-spec: $(EMILY_OPENAPI_SPEC)
emily-curl-test:
	./devenv/service-test/curl-test.sh localhost 3031 0
emily-server:
	cargo run --bin emily-server

.PHONY: emily-lambda emily-cdk-synth emily-openapi-spec emily-curl-test emily-server

# Generate the client code using the OpenAPI spec
$(AUTOGENERATED_EMILY_CLIENT): $(EMILY_OPENAPI_SPEC)
	@echo "Building emily client from Openapi Spec"
	pnpm --prefix $(EMILY_OPENAPI_PATH) run build

# Generate the OpenAPI spec for Emily Client
$(EMILY_OPENAPI_SPEC): $(INSTALL_TARGET) $(filter-out $(EMILY_OPENAPI_SPEC), $(wildcard $(subst dir, $(EMILY_OPENAPI_PATH), $(THREE_DIRS_DEEP))))
	cargo build --package emily-openapi-spec

# Blocklist Client API
# ----------------------------------------------------

emily-client: $(AUTOGENERATED_EMILY_CLIENT)

.PHONY: emily-client emily-client-test

# Generate the client code using the OpenAPI spec
$(AUTOGENERATED_BLOCKLIST_CLIENT_CLIENT): $(BLOCKLIST_OPENAPI_SPEC)
	pnpm --prefix $(BLOCKLIST_OPENAPI_PATH) run build

# Generate the OpenAPI spec for Blocklist Client
$(BLOCKLIST_OPENAPI_SPEC): $(INSTALL_TARGET) $(filter-out $(BLOCKLIST_OPENAPI_SPEC), $(wildcard $(subst dir, $(BLOCKLIST_OPENAPI_PATH), $(THREE_DIRS_DEEP))))
	cargo build --package blocklist-openapi-gen

# Local Docker Compose Devenv

devenv-up: $(INSTALL_TARGET) $(EMILY_LAMBDA_BINARY) $(EMILY_CDK_TEMPLATE)
	cd devenv/local/docker-compose && \
	CONTAINER_HOST=$(_CONTAINER_HOST) sh up.sh

devenv-down:
	cd devenv/local/docker-compose && \
	CONTAINER_HOST=$(_CONTAINER_HOST) sh down.sh
