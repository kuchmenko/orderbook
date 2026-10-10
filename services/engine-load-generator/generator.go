package main

import (
	"fmt"
	"time"
)

type GeneratorState struct {
	LastID EpochSequenceID
}

type EpochSequenceID struct {
	Epoch    uint32 `json:"epoch"`
	Sequence uint64 `json:"sequence"`
}

func (es EpochSequenceID) String() string {
	return fmt.Sprintf("%d:%d", es.Epoch, es.Sequence)
}

type Generator struct {
	epoch    uint32
	sequence uint64
}

func (g *Generator) Next() EpochSequenceID {
	g.sequence++

	return EpochSequenceID{
		Epoch:    g.epoch,
		Sequence: g.sequence,
	}
}

func (g *Generator) Last() EpochSequenceID {
	return EpochSequenceID{
		Epoch:    g.epoch,
		Sequence: g.sequence,
	}
}

func NewGenerator(sequence uint64) Generator {
	return Generator{
		epoch:    uint32(time.Now().UnixMicro()),
		sequence: sequence,
	}
}
