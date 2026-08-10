// PRODUCTION TWIN of services/handler_test.go — byte-for-byte the same defects on a path that is not a
// test path. Its findings are expectations in EXPECTED.jsonc; the twin's silence is a `benign` entry.
// Neither half proves anything alone: without this file the twin could be silent because the content
// stopped triggering, and without the twin this file could pass while the path gate was gone.
package services

import "fmt"

// Fans out one unbounded goroutine per iteration, and logs to stdout from inside the loop.
func FanOut(rows []string) {
	for _, row := range rows {
		go func() { fmt.Println("worker: " + row) }()
		fmt.Println("dispatched: " + row)
	}
}
