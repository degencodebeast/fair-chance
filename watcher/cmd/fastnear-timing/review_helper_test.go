package main

import (
	"fmt"

	"fairchance/watcher/internal/timing"
)

func tSeen(i int) timing.Seen {
	return timing.Seen{Event: "fc_buy", ReceiptID: fmt.Sprintf("r%d", i), TxHash: fmt.Sprintf("t%d", i)}
}
