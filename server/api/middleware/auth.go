package middleware

import (
	"context"

	apiutils "github.com/darkpool-exchange/server/api/utils"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/metadata"
	"google.golang.org/grpc/status"
)

const authHeader = "x-api-key"

type AuthInterceptor struct {
	validKeys map[string]bool
}

func NewAuthInterceptor(keys []string) *AuthInterceptor {
	m := make(map[string]bool, len(keys))
	for _, k := range keys {
		m[k] = true
	}
	return &AuthInterceptor{validKeys: m}
}

func (a *AuthInterceptor) Unary() grpc.UnaryServerInterceptor {
	return func(ctx context.Context, req any, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (any, error) {
		if err := a.authorize(ctx); err != nil {
			return nil, err
		}
		return handler(ctx, req)
	}
}

func (a *AuthInterceptor) Stream() grpc.StreamServerInterceptor {
	return func(srv any, ss grpc.ServerStream, info *grpc.StreamServerInfo, handler grpc.StreamHandler) error {
		if err := a.authorize(ss.Context()); err != nil {
			return err
		}
		return handler(srv, ss)
	}
}

func (a *AuthInterceptor) authorize(ctx context.Context) error {
	if len(a.validKeys) == 0 {
		return nil // no keys configured = auth disabled
	}

	md, ok := metadata.FromIncomingContext(ctx)
	if !ok {
		return status.Error(codes.Unauthenticated, apiutils.MsgMissingMetadata)
	}

	keys := md.Get(authHeader)
	if len(keys) == 0 {
		return status.Error(codes.Unauthenticated, apiutils.MsgMissingAPIKey)
	}

	if !a.validKeys[keys[0]] {
		return status.Error(codes.PermissionDenied, apiutils.MsgInvalidAPIKey)
	}

	return nil
}
