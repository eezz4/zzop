// be-reliability/console-in-loop (Go lane) — bad: a fmt write the parser places INSIDE a for
// statement's projected span, so it runs once per iteration. good: one aggregated write after the
// loop — that line still co-fires console-in-be (this file sits under services/), which is a
// different claim (WHERE the write is, not how many times it runs); same pairing as the TS fixture.
package services

import "fmt"

func BadLoop(orderIds []string) {
	for _, id := range orderIds {
		fmt.Println("processing order " + id)
	}
}

func GoodAfter(orderIds []string) {
	for _, id := range orderIds {
		accumulate(id)
	}
	fmt.Println(len(orderIds))
}

func accumulate(id string) {}
