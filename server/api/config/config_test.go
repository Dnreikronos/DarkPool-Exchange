package config

import (
	"testing"
	"time"
)

func TestEnvOr(t *testing.T) {
	t.Setenv("DARKPOOL_TEST_STR", "from-env")

	if got := envOr("cli-val", "", "DARKPOOL_TEST_STR"); got != "cli-val" {
		t.Fatalf("flag overrides env: want cli-val, got %q", got)
	}
	if got := envOr("", "", "DARKPOOL_TEST_STR"); got != "from-env" {
		t.Fatalf("env fills default: want from-env, got %q", got)
	}
	if got := envOr("", "", "DARKPOOL_TEST_MISSING"); got != "" {
		t.Fatalf("no flag + no env = default: got %q", got)
	}
}

func TestEnvUint64(t *testing.T) {
	t.Setenv("DARKPOOL_TEST_UINT", "42")

	if got := envUint64(7, 0, "DARKPOOL_TEST_UINT"); got != 7 {
		t.Fatalf("flag overrides env: want 7, got %d", got)
	}
	if got := envUint64(0, 0, "DARKPOOL_TEST_UINT"); got != 42 {
		t.Fatalf("env fills default: want 42, got %d", got)
	}

	t.Setenv("DARKPOOL_TEST_UINT", "not-a-number")
	if got := envUint64(0, 0, "DARKPOOL_TEST_UINT"); got != 0 {
		t.Fatalf("bad env falls back to default: got %d", got)
	}
}

func TestEnvDuration(t *testing.T) {
	t.Setenv("DARKPOOL_TEST_DUR", "2m30s")

	if got := envDuration(0, 0, "DARKPOOL_TEST_DUR"); got != 2*time.Minute+30*time.Second {
		t.Fatalf("env fills default: got %v", got)
	}
	if got := envDuration(10*time.Second, 0, "DARKPOOL_TEST_DUR"); got != 10*time.Second {
		t.Fatalf("flag overrides env: got %v", got)
	}
}
