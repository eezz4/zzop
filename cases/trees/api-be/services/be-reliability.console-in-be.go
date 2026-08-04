// be-reliability/console-in-be (Go lane) — bad: fmt console writes in backend-path source, including
// the Fprint* form whose writer is SPELLED os.Stderr at the site. good: the stdlib log package —
// deliberately NOT folded into console-write (log.SetOutput retargets it process-wide, i.e. it is a
// configurable logger; the design doc's named boundary case, disclosed in the Go producer's module doc).
package services

import (
	"fmt"
	"log"
	"os"
)

func BadPrintln() {
	fmt.Println("processing request")
}

func BadStderr() {
	fmt.Fprintf(os.Stderr, "failed: %d\n", 1)
}

func GoodLogger() {
	log.Println("processing request")
}
