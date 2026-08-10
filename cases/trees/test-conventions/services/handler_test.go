// TEST TWIN of services/handler.go — the Go convention (`_test.go`), which the DSL's `${test-paths}`
// vocabulary did not match until 2026-08-10. Every finding the production twin carries is expected to be
// ABSENT here, and any finding at all scores as a false positive (`benign` in EXPECTED.jsonc).
//
// The defects below are deliberately identical to the twin's, so a divergence can only come from the
// PATH. The `services/` directory is load-bearing too: `reliability/console-in-be` is gated on a backend
// path segment, so without it this file's silence under that rule would be free.
package services

import (
	"fmt"
	"testing"
)

func TestFanOut(t *testing.T) {
	rows := []string{"a", "b"}
	for _, row := range rows {
		go func() { fmt.Println("worker: " + row) }()
		fmt.Println("dispatched: " + row)
	}
}
