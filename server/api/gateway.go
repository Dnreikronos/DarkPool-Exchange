package api

import (
	"context"
	"fmt"
	"net/http"

	"github.com/grpc-ecosystem/grpc-gateway/v2/runtime"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	darkpoolv1 "github.com/darkpool-exchange/server/api/gen/darkpool/v1"
)

// NewGateway creates an HTTP handler that proxies REST requests to the gRPC server.
func NewGateway(ctx context.Context, grpcAddr string) (http.Handler, error) {
	mux := runtime.NewServeMux()

	opts := []grpc.DialOption{grpc.WithTransportCredentials(insecure.NewCredentials())}

	if err := darkpoolv1.RegisterDarkPoolServiceHandlerFromEndpoint(ctx, mux, grpcAddr, opts); err != nil {
		return nil, fmt.Errorf("failed to register gateway: %w", err)
	}

	return mux, nil
}
