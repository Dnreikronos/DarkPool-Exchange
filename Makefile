# Thin wrapper: delegates to justfile. See `just --list` for the full menu.
# Kept for muscle memory / contributors who reach for `make` first.

.PHONY: build test up up-zk down clean

build:
	just build-all

test:
	just test-full

up:
	just up

up-zk:
	just up-zk

down:
	just down

clean:
	just clean
