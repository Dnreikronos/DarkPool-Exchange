package middleware

import (
	"context"
	"fmt"
	"testing"

	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/metadata"
	"google.golang.org/grpc/status"
)

// ---------------------------------------------------------------------------
// helpers (shared with ratelimit_test.go via same package)
// ---------------------------------------------------------------------------

func ctxWithAPIKey(key string) context.Context {
	return metadata.NewIncomingContext(
		context.Background(),
		metadata.Pairs("x-api-key", key),
	)
}

func ctxWithEmptyMetadata() context.Context {
	return metadata.NewIncomingContext(context.Background(), metadata.MD{})
}

func dummyUnaryHandler(_ context.Context, _ any) (any, error) {
	return "ok", nil
}

type mockServerStream struct {
	grpc.ServerStream
	ctx context.Context
}

func (m *mockServerStream) Context() context.Context { return m.ctx }

func dummyStreamHandler(_ any, _ grpc.ServerStream) error {
	return nil
}

func assertCode(t *testing.T, err error, want codes.Code) {
	t.Helper()
	if err == nil {
		t.Fatalf("expected error with code %v, got nil", want)
	}
	st, ok := status.FromError(err)
	if !ok {
		t.Fatalf("expected gRPC status error, got %T: %v", err, err)
	}
	if st.Code() != want {
		t.Errorf("code = %v, want %v (msg: %s)", st.Code(), want, st.Message())
	}
}

// ---------------------------------------------------------------------------
// Auth unary tests
// ---------------------------------------------------------------------------

func TestAuth_Unary_NoKeysConfigured(t *testing.T) {
	a := NewAuthInterceptor(nil)
	interceptor := a.Unary()
	resp, err := interceptor(context.Background(), nil, nil, dummyUnaryHandler)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp != "ok" {
		t.Errorf("response = %v, want ok", resp)
	}
}

func TestAuth_Unary_ValidKey(t *testing.T) {
	a := NewAuthInterceptor([]string{"secret-1", "secret-2"})
	interceptor := a.Unary()
	resp, err := interceptor(ctxWithAPIKey("secret-1"), nil, nil, dummyUnaryHandler)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp != "ok" {
		t.Errorf("response = %v, want ok", resp)
	}
}

func TestAuth_Unary_InvalidKey(t *testing.T) {
	a := NewAuthInterceptor([]string{"secret-1"})
	interceptor := a.Unary()
	_, err := interceptor(ctxWithAPIKey("wrong"), nil, nil, dummyUnaryHandler)
	assertCode(t, err, codes.PermissionDenied)
}

func TestAuth_Unary_MissingMetadata(t *testing.T) {
	a := NewAuthInterceptor([]string{"secret-1"})
	interceptor := a.Unary()
	_, err := interceptor(context.Background(), nil, nil, dummyUnaryHandler)
	assertCode(t, err, codes.Unauthenticated)
}

func TestAuth_Unary_MissingAPIKeyHeader(t *testing.T) {
	a := NewAuthInterceptor([]string{"secret-1"})
	interceptor := a.Unary()
	_, err := interceptor(ctxWithEmptyMetadata(), nil, nil, dummyUnaryHandler)
	assertCode(t, err, codes.Unauthenticated)
}

// ---------------------------------------------------------------------------
// Auth stream tests
// ---------------------------------------------------------------------------

func TestAuth_Stream_ValidKey(t *testing.T) {
	a := NewAuthInterceptor([]string{"key-1"})
	interceptor := a.Stream()
	ss := &mockServerStream{ctx: ctxWithAPIKey("key-1")}
	err := interceptor(nil, ss, nil, dummyStreamHandler)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestAuth_Stream_InvalidKey(t *testing.T) {
	a := NewAuthInterceptor([]string{"key-1"})
	interceptor := a.Stream()
	ss := &mockServerStream{ctx: ctxWithAPIKey("wrong")}
	err := interceptor(nil, ss, nil, dummyStreamHandler)
	assertCode(t, err, codes.PermissionDenied)
}

func TestAuth_Stream_MissingMetadata(t *testing.T) {
	a := NewAuthInterceptor([]string{"key-1"})
	interceptor := a.Stream()
	ss := &mockServerStream{ctx: context.Background()}
	err := interceptor(nil, ss, nil, dummyStreamHandler)
	assertCode(t, err, codes.Unauthenticated)
}

// ---------------------------------------------------------------------------
// Multiple keys (table-driven)
// ---------------------------------------------------------------------------

func TestAuth_Unary_MultipleKeys(t *testing.T) {
	a := NewAuthInterceptor([]string{"key-a", "key-b", "key-c"})
	interceptor := a.Unary()

	tests := []struct {
		name    string
		key     string
		wantErr bool
		code    codes.Code
	}{
		{"first key", "key-a", false, 0},
		{"second key", "key-b", false, 0},
		{"third key", "key-c", false, 0},
		{"invalid key", "key-d", true, codes.PermissionDenied},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := interceptor(ctxWithAPIKey(tt.key), nil, nil, dummyUnaryHandler)
			if tt.wantErr {
				assertCode(t, err, tt.code)
			} else if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// Handler error propagation
// ---------------------------------------------------------------------------

func TestAuth_Unary_HandlerErrorPropagates(t *testing.T) {
	a := NewAuthInterceptor([]string{"key-1"})
	interceptor := a.Unary()

	handlerErr := fmt.Errorf("handler boom")
	failHandler := func(_ context.Context, _ any) (any, error) {
		return nil, handlerErr
	}

	_, err := interceptor(ctxWithAPIKey("key-1"), nil, nil, failHandler)
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if err.Error() != handlerErr.Error() {
		t.Errorf("error = %v, want %v", err, handlerErr)
	}
}
