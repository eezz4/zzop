// DECOY — no rule may fire anywhere in this file (a finding here is a false positive by definition).
// Near-misses for the console-write family, each a boundary the Go producer's module doc names:
// the stdlib log package written INSIDE a loop (a configurable logger — the design doc's excluded
// boundary case, not a console write even though it defaults to stderr), fmt.Sprintf (builds a
// string, writes nothing), and fmt.Fprintf to a non-console writer.
package lib

import (
	"bytes"
	"fmt"
	"log"
)

func LogInLoop(rows []string) {
	for _, row := range rows {
		log.Println("row: " + row)
	}
}

func SprintfOnly(n int) string {
	return fmt.Sprintf("count=%d", n)
}

func BufferWrite(buf *bytes.Buffer, msg string) {
	fmt.Fprintf(buf, "msg=%s\n", msg)
}
