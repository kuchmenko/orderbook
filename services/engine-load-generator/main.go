package main

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/google/uuid"
	"github.com/twmb/franz-go/pkg/kgo"
)

type EpochSequenceID struct {
	Epoch    uint32 `json:"epoch"`
	Sequence uint64 `json:"sequence"`
}

func (es EpochSequenceID) String() string {
	return fmt.Sprintf("%d:%d", es.Epoch, es.Sequence)
}

type (
	MarketID uuid.UUID
	OrderID  EpochSequenceID
)

func (id MarketID) MarshalText() ([]byte, error) {
	return uuid.UUID(id).MarshalText()
}

type (
	Price  uint32
	Amount uint32
)

type Side int

const (
	SideBuy  Side = 0
	SideSell Side = 1
)

type CreateMarket struct {
	MarketID MarketID `json:"market_id"`
}

type OrderIntent struct {
	Side   Side   `json:"side"`
	Price  Price  `json:"price"`
	Amount Amount `json:"amount"`
}
type PlaceOrder struct {
	MarketID MarketID    `json:"market_id"`
	Intent   OrderIntent `json:"intent"`
}
type CancelOrder struct {
	MarketID MarketID `json:"market_id"`
	OrderID  OrderID  `json:"order_id"`
}

type InputID EpochSequenceID

type InputKind struct {
	CreateMarket *CreateMarket `json:"CreateMarket,omitempty"`
	PlaceOrder   *PlaceOrder   `json:"PlaceOrder,omitempty"`
	CancelOrder  *CancelOrder  `json:"CancelOrder,omitempty"`
}

type Input struct {
	ID   InputID   `json:"id"`
	Kind InputKind `json:"kind"`
}

func sendInput(ctx context.Context, client *kgo.Client, input Input) error {
	payload, err := json.Marshal(input)
	if err != nil {
		return nil
	}

	return client.ProduceSync(ctx, &kgo.Record{
		Topic: "engine-input",
		Value: payload,
	}).FirstErr()
}

func main() {
	ctx, _ := context.WithDeadline(context.Background(), time.Now().Add(2*time.Second))
	client, err := kgo.NewClient(
		kgo.SeedBrokers("127.0.0.1:9092"),
	)
	if err != nil {
		fmt.Sprintf("kafka client: %v", err)
		return
	}
	defer client.Close()

	newMarketID, err := uuid.NewV7()
	if err != nil {
		fmt.Sprintf("market uuid creation: %v", err)
		return
	}

	input := Input{
		ID: InputID{
			Epoch:    0,
			Sequence: 0,
		},
		Kind: InputKind{
			CreateMarket: &CreateMarket{
				MarketID: MarketID(newMarketID),
			},
		},
	}

	inputJson, err := json.Marshal(input)
	if err != nil {
		fmt.Sprintf("input json marshal: %v", err)
		return
	}

	fmt.Printf("Hello world\n %v\n%s\n", input, inputJson)

	err = sendInput(ctx, client, input)
	if err != nil {
		fmt.Sprintf("send input: %v", err)
		return
	}
	fmt.Printf("Sent input\n")
}
