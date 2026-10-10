package main

import (
	"context"
	"encoding/json"
	"fmt"
	"math/rand/v2"
	"os"
	"os/signal"
	"sync"
	"syscall"

	"github.com/google/uuid"
	"github.com/twmb/franz-go/pkg/kgo"
)

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

type Side string

const (
	SideBuy  Side = "Buy"
	SideSell Side = "Sell"
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

type Fill struct {
	OrderID     OrderID `json:"id"`
	Price       Price   `json:"price"`
	TotalAmount Amount  `json:"totla_amount"`
	Amount      Amount  `json:"amount"`
}

type Order struct {
	ID     OrderID `json:"id"`
	Side   Side    `json:"side"`
	Price  Price   `json:"price"`
	Amount Amount  `json:"amount"`
}

type OrderFill struct {
	Order Order `json:"order"`
	Fill  Fill  `json:"fill"`
}

type OrderPlaced struct {
	MarketID MarketID     `json:"market_id"`
	Order    Order        `json:"order"`
	Fills    *[]OrderFill `json:"fills,omitempty"`
}

type OrderCanceled struct {
	MarketID MarketID `json:"market_id"`
	Order    *Order   `json:"order,omitempty"`
}

type MarketCreated struct {
	MarketID MarketID `json:"market_id"`
}

type OutputID EpochSequenceID

type OutputKind struct {
	OrderPlaced   *OrderPlaced   `json:"OrderPlaced,omitempty"`
	OrderCanceled *OrderCanceled `json:"OrderCanceled,omitempty"`
	MarketCreated *MarketCreated `json:"MarketCreated,omitempty"`
}

type Output struct {
	ID      OutputID   `json:"id"`
	InputID InputID    `json:"id"`
	Kind    OutputKind `json:"kind"`
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

func generateActions(ctx context.Context, idGenerator Generator, markets []MarketID, output chan Input) error {
	defer close(output)
	for {
		select {
		case <-ctx.Done():
			return nil
		default:
		}

		inputID := idGenerator.Next()
		market := markets[rand.IntN(len(markets))]
		var side Side

		if rand.IntN(10000)%2 == 0 {
			side = SideBuy
		} else {
			side = SideSell
		}

		price := Price(rand.IntN(10000))
		amount := Amount(rand.IntN(10000))

		input := InputKind{
			PlaceOrder: &PlaceOrder{
				MarketID: MarketID(market),
				Intent: OrderIntent{
					Side:   side,
					Price:  price,
					Amount: amount,
				},
			},
		}

		output <- Input{
			ID:   InputID(inputID),
			Kind: input,
		}

	}

	return nil
}

func generateMarkets() ([]MarketID, error) {
	res := make([]MarketID, 0, 10)
	for range 10 {
		id, err := uuid.NewV7()
		if err != nil {
			return nil, err
		}

		res = append(res, MarketID(id))
	}

	return res, nil
}

func main() {
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	client, err := kgo.NewClient(
		kgo.SeedBrokers("127.0.0.1:9092"),
	)
	if err != nil {
		fmt.Sprintf("kafka client: %v", err)
		return
	}
	defer client.Close()

	seq := uint64(0)
	markets, err := generateMarkets()
	if err != nil {
		fmt.Sprintf("markets generation: %v", err)
		return
	}
	actions := make(chan Input, 100)

	var wg sync.WaitGroup

	wg.Go(func() {
		idGenerator := NewGenerator(0)
		generateActions(ctx, idGenerator, markets, actions)
	})

	wg.Go(func() {
		counter := 0
		for input := range actions {
			if counter >= 100 {
				cancel()
				return
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

			seq += 1
			counter++
		}
	})
	wg.Wait()
}
