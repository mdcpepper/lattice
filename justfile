set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

ext_so := "target/debug/liblattice.so"

default: test

cloc:
    cloc fixtures crates php --exclude_dir vendor,dist

test-rust:
    cargo test --all-features --workspace --exclude lattice-php-ext

build-extension:
    cd crates/php-ext && cargo build

test-extension: build-extension
    cd php && php -d extension=../{{ ext_so }} vendor/bin/pest --configuration=phpunit.xml

test: test-rust test-extension

watch:
    docker compose --profile dev up json-api-dev
