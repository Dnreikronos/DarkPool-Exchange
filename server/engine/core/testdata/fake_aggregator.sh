#!/bin/sh
# Test fixture: echo deterministic proof bytes. Ignores stdin payload.
printf 'fake-proof-%s' "$(cat | wc -c | tr -d ' ')"
