package core

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"time"

	"github.com/darkpool-exchange/server/engine/event"
	"github.com/google/uuid"
)

// SubprocessAggregator shells out to a Rust aggregator CLI. Protocol:
//
//	stdin : JSON {"batch_id": "...", "matches": [...]}  (see aggregatorInput)
//	stdout: raw proof bytes (no framing, no encoding)
//	exit 0 = success; anything else is an error and stderr is included in err.
//
// The CLI must honor the provided context for cancellation by exiting
// promptly when its stdin is closed or the process is killed.
type SubprocessAggregator struct {
	BinPath string
	Timeout time.Duration
}

type aggregatorMatch struct {
	AuctionID string `json:"auction_id"`
	BidID     string `json:"bid_order_id"`
	AskID     string `json:"ask_order_id"`
	Price     string `json:"price"`
	Size      string `json:"size"`
}

type aggregatorInput struct {
	BatchID string            `json:"batch_id"`
	Matches []aggregatorMatch `json:"matches"`
}

// NewSubprocessAggregator validates the binary exists and is executable so
// the engine fails loudly at startup rather than on the first auction tick.
func NewSubprocessAggregator(binPath string, timeout time.Duration) (*SubprocessAggregator, error) {
	if binPath == "" {
		return nil, fmt.Errorf("aggregator bin path required")
	}
	info, err := os.Stat(binPath)
	if err != nil {
		return nil, fmt.Errorf("stat aggregator bin: %w", err)
	}
	if info.IsDir() {
		return nil, fmt.Errorf("aggregator bin %q is a directory", binPath)
	}
	if info.Mode()&0o111 == 0 {
		return nil, fmt.Errorf("aggregator bin %q is not executable", binPath)
	}
	if timeout <= 0 {
		timeout = 30 * time.Second
	}
	return &SubprocessAggregator{BinPath: binPath, Timeout: timeout}, nil
}

func (s *SubprocessAggregator) Aggregate(ctx context.Context, batchID uuid.UUID, matches []event.OrderMatched) ([]byte, error) {
	in := aggregatorInput{
		BatchID: batchID.String(),
		Matches: make([]aggregatorMatch, 0, len(matches)),
	}
	for _, m := range matches {
		in.Matches = append(in.Matches, aggregatorMatch{
			AuctionID: m.AuctionID.String(),
			BidID:     m.Bid.OrderID.String(),
			AskID:     m.Ask.OrderID.String(),
			Price:     m.Price.String(),
			Size:      m.Size.String(),
		})
	}
	payload, err := json.Marshal(in)
	if err != nil {
		return nil, fmt.Errorf("marshal aggregator input: %w", err)
	}

	cctx, cancel := context.WithTimeout(ctx, s.Timeout)
	defer cancel()

	cmd := exec.CommandContext(cctx, s.BinPath)
	// Without WaitDelay, an orphaned grandchild (e.g. a shell that forked
	// `sleep`) keeps the stdout pipe open after we SIGKILL the direct child,
	// and cmd.Run() blocks until the grandchild exits on its own. 500ms is
	// enough for a cooperative child to flush.
	cmd.WaitDelay = 500 * time.Millisecond
	cmd.Stdin = bytes.NewReader(payload)

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		if cctx.Err() == context.DeadlineExceeded {
			return nil, fmt.Errorf("aggregator timed out after %s: %s", s.Timeout, stderr.String())
		}
		if exitErr, ok := err.(*exec.ExitError); ok {
			return nil, fmt.Errorf("aggregator exit %d: %s", exitErr.ExitCode(), stderr.String())
		}
		return nil, fmt.Errorf("aggregator run: %w (stderr=%s)", err, stderr.String())
	}

	proof := stdout.Bytes()
	if len(proof) == 0 {
		return nil, fmt.Errorf("aggregator produced empty proof (stderr=%s)", stderr.String())
	}
	// Drop trailing newline a shell script is likely to append so downstream
	// byte comparisons against a fixture stay stable.
	proof = bytes.TrimRight(proof, "\n")
	return append([]byte(nil), proof...), nil
}

