/**
 * A small Go service for browser mode, opened with `?project=go`: `ledger`, the module
 * `github.com/acme/ledger` with a money package (tests, subtests, a benchmark, a fuzz test
 * and an example), a report package with a `//go:generate` directive and a store with
 * something for golangci-lint to find. goMock.ts answers the `go_*` commands, running the
 * Rust crates through WebAssembly.
 */
import type { ProjectInfo } from "@/ipc/project";

export const goDemoName = "ledger";
export const GO_MODULE = "github.com/acme/ledger";

/** True when the page asks for the Go demo project. */
export const isGoDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "go";

export const goDemoInfo: ProjectInfo = {
  languages: ["go"],
  frameworks: [],
  packs: ["goforge"],
  tools: ["git", "docker"],
  primary: { id: "go", name: "Go", language: "go" },
};

const GO_MOD = `module github.com/acme/ledger

go 1.23.0

toolchain go1.24.7

require (
	github.com/google/uuid v1.6.0
	golang.org/x/text v0.21.0
)

require github.com/stretchr/testify v1.10.0 // indirect

replace github.com/acme/ledger/legacy => ./legacy
`;

const GO_SUM = `github.com/google/uuid v1.6.0 h1:NIvaJDMOsjHA8n1jAhLSgzrAzy1Hgr+hNrb57e+94F0=
github.com/google/uuid v1.6.0/go.mod h1:TIyPZe4MgqvfeYDBFedMoGGpEw/LqOeaOT+nhxU+yHo=
github.com/stretchr/testify v1.10.0 h1:Xv5erBjTwe/5IxqUQTdXv5kgmIvbHo3QQyRwhJsOfJA=
github.com/stretchr/testify v1.10.0/go.mod h1:r2ic/lqez/lEtzL7wO/rwa5dbSLXVDPFyf8C91i36aY=
golang.org/x/text v0.21.0 h1:zyQAAkrwaneQ066sspRyJaG9VNi/YJ1NfzcGB3hZ/qo=
golang.org/x/text v0.21.0/go.mod h1:4IBbMaMmOPCJ8SecivzSH54+73PCFmPWxNTLm+vZkEQ=
`;

const MAIN = `// Command ledger prints the monthly report of a ledger file.
package main

import (
	"flag"
	"fmt"
	"log"
	"os"

	"github.com/acme/ledger/internal/report"
	"github.com/acme/ledger/internal/store"
)

func main() {
	path := flag.String("file", "ledger.csv", "the ledger to read")
	month := flag.String("month", "2026-10", "the month to report")
	flag.Parse()

	entries, err := store.Load(*path)
	if err != nil {
		log.Fatalf("cannot read %s: %v", *path, err)
	}
	lines := report.Render(entries, *month)
	for _, line := range lines {
		fmt.Fprintln(os.Stdout, line)
	}
}
`;

const MONEY = `// Package money stores amounts in cents.
package money

import (
	"fmt"
	"strconv"
	"strings"
)

// Amount is a sum of money in cents.
type Amount int64

// Parse reads an amount such as "12.34".
func Parse(s string) (Amount, error) {
	whole, frac, _ := strings.Cut(strings.TrimSpace(s), ".")
	w, err := strconv.ParseInt(whole, 10, 64)
	if err != nil {
		return 0, fmt.Errorf("parse %q: %w", s, err)
	}
	f, _ := strconv.ParseInt((frac + "00")[:2], 10, 64)
	if w < 0 {
		f = -f
	}
	return Amount(w*100 + f), nil
}

// Round rounds to whole units, half up.
func Round(a Amount) Amount {
	return (a + 50) / 100 * 100
}

// String formats the amount with two decimals.
func (a Amount) String() string {
	return fmt.Sprintf("%d.%02d", a/100, a%100)
}

// Sum adds amounts.
func Sum(amounts ...Amount) Amount {
	var total Amount
	for _, a := range amounts {
		total += a
	}
	return total
}
`;

const MONEY_TEST = `package money

import (
	"fmt"
	"testing"
)

func TestRound(t *testing.T) {
	t.Run("half up", func(t *testing.T) {
		if got := Round(150); got != 200 {
			t.Errorf("got %v, want 2.00", got)
		}
	})
	t.Run("negative", func(t *testing.T) {
		if got := Round(-150); got != -200 {
			t.Errorf("got %v, want -2.00", got)
		}
	})
	t.Run("zero", func(t *testing.T) {
		if got := Round(0); got != 0 {
			t.Errorf("got %v, want 0.00", got)
		}
	})
}

func TestParse(t *testing.T) {
	a, err := Parse("12.34")
	if err != nil || a != 1234 {
		t.Fatalf("got %v %v", a, err)
	}
}

func TestSum(t *testing.T) {
	if got := Sum(1250, 725); got != 1975 {
		t.Errorf("got %v, want 19.75", got)
	}
}

func BenchmarkString(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = Amount(123456).String()
	}
}

func FuzzParse(f *testing.F) {
	f.Add("1.5")
	f.Add("-3.25")
	f.Fuzz(func(t *testing.T, s string) {
		_, _ = Parse(s)
	})
}

func ExampleAmount_String() {
	fmt.Println(Amount(1999))
	// Output: 19.99
}
`;

const REPORT = `// Package report renders the monthly report.
package report

import (
	"fmt"
	"strings"

	"github.com/acme/ledger/internal/money"
	"github.com/acme/ledger/internal/store"
)

//go:generate stringer -type=Kind -output=kind_string.go

// Kind of a report line.
type Kind int

// Total adds the entries of a month.
func Total(entries []store.Entry, month string) money.Amount {
	var total money.Amount
	for _, e := range entries {
		if strings.HasPrefix(e.Date, month) {
			total += e.Amount
		}
	}
	return money.Round(total)
}

// Render returns the report lines of a month.
func Render(entries []store.Entry, month string) []string {
	lines := []string{fmt.Sprintf("Ledger %s", month)}
	for _, e := range entries {
		if !strings.HasPrefix(e.Date, month) {
			continue
		}
		lines = append(lines, formatLine(e))
	}
	return append(lines, fmt.Sprintf("Total %s", Total(entries, month)))
}

const (
	// Income is money in.
	Income Kind = iota
	// Expense is money out.
	Expense
)

func kindOf(a money.Amount) Kind {
	if a < 0 {
		return Expense
	}
	return Income
}

func formatLine(e store.Entry) string {
	return fmt.Sprintf("%s  %-24s %10s  %v", e.Date, e.Text, e.Amount.String(), kindOf(e.Amount))
}
`;

const REPORT_TEST = `package report

import (
	"testing"

	"github.com/acme/ledger/internal/store"
)

var entries = []store.Entry{
	{Date: "2026-10-01", Text: "Rent", Amount: -120000},
	{Date: "2026-10-03", Text: "Invoice 1042", Amount: 349900},
	{Date: "2026-11-01", Text: "Rent", Amount: -120000},
}

func TestRender(t *testing.T) {
	lines := Render(entries, "2026-10")
	if len(lines) != 4 {
		t.Fatalf("got %d lines, want 4", len(lines))
	}
}

func TestTotal(t *testing.T) {
	if got := Total(entries, "2026-10"); got != 229900 {
		t.Errorf("got %v, want 2299.00", got)
	}
}

func BenchmarkReport(b *testing.B) {
	for i := 0; i < b.N; i++ {
		Render(entries, "2026-10")
	}
}
`;

const STORE = `// Package store reads ledger files.
package store

import (
	"encoding/csv"
	"os"

	"github.com/acme/ledger/internal/money"
)

// Entry is one booking.
type Entry struct {
	Date   string
	Text   string
	Amount money.Amount
}

// Load reads a CSV ledger: date, text, amount.
func Load(path string) ([]Entry, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	rows, err := csv.NewReader(f).ReadAll()
	if err != nil {
		return nil, err
	}
	var out []Entry
	for _, r := range rows {
		a, _ := money.Parse(r[2])
		out = append(out, Entry{Date: r[0], Text: r[1], Amount: a})
	}
	return out, nil
}

// Save writes the entries back.
func Save(path string, entries []Entry) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	w := csv.NewWriter(f)
	for _, e := range entries {
		w.Write([]string{e.Date, e.Text, e.Amount.String()})
	}
	w.Flush()
	return f.Close()
}
`;

const GOLANGCI = `version: "2"
linters:
  default: standard
  enable:
    - errcheck
    - govet
    - staticcheck
    - unused
`;

const README = `# ledger

Monthly reports of a CSV ledger.

    go run ./cmd/ledger -file ledger.csv -month 2026-10
    go test ./...
`;

export const goDemoFiles: Record<string, string> = {
  "go.mod": GO_MOD,
  "go.sum": GO_SUM,
  ".golangci.yml": GOLANGCI,
  "README.md": README,
  ".gitignore": "/ledger\n*.pprof\ncoverage.out\n",
  "ledger.csv": "2026-10-01,Rent,-1200.00\n2026-10-03,Invoice 1042,3499.00\n",
  "cmd/ledger/main.go": MAIN,
  "internal/money/money.go": MONEY,
  "internal/money/money_test.go": MONEY_TEST,
  "internal/report/report.go": REPORT,
  "internal/report/report_test.go": REPORT_TEST,
  "internal/store/store.go": STORE,
};
